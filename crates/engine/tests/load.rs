//! Independent ELF fixtures and linked images verify loading rather than section projections.

use object::{Object, ObjectSection};
use oplab_core::{
    address::Address, diagnostic::ValidationError, memory::MAX_MAPPED_BYTES, target::Target,
};
use oplab_engine::{
    load::{LoadError, LoadPlan},
    machine::Machine,
};
use oplab_toolchain::assembly;
use proptest::prelude::*;

proptest! {
    #[test]
    fn arbitrary_segment_tails_stay_zero_and_inside_page_envelopes(
        offset in 0_u16..4096,
        data in proptest::collection::vec(any::<u8>(), 0..128),
        tail in 1_u16..8192,
        page_shift in 8_u32..=12,
    ) {
        let page_size = 1_u64 << page_shift;
        let start = 0x1000 + u64::from(offset);
        let length = data.len() as u64 + u64::from(tail);
        let mut bytes = image(start, &[[start, start, data.len() as u64, length, 5, 1]])?;
        let file_start = 0x1000 + usize::from(offset);
        bytes[file_start..file_start + data.len()].copy_from_slice(&data);
        let plan = LoadPlan::from_elf(&bytes, Target::X86_64, page_size)?;
        let regions = plan.memory().regions();
        prop_assert_eq!(regions.len(), 1);
        let region = &regions[0];
        let begin = region.range().start().get();
        prop_assert!(begin <= start && start - begin < page_size);
        prop_assert!(region.range().end() >= u128::from(start + length));
        prop_assert!(region.range().end() - u128::from(start + length) < u128::from(page_size));
        let prefix = usize::try_from(start - begin)?;
        let mut contents = region.initial().to_vec();
        contents.resize(usize::try_from(region.range().length())?, 0);
        prop_assert_eq!(&contents[prefix..prefix + data.len()], data.as_slice());
        prop_assert!(contents[..prefix].iter().all(|byte| *byte == 0));
        prop_assert!(contents[prefix + data.len()..].iter().all(|byte| *byte == 0));
    }
}

// These fixtures use ELF64's published field offsets, independent of the production parser.
fn image(entry: u64, segments: &[[u64; 6]]) -> Result<Vec<u8>, std::num::TryFromIntError> {
    let mut image = vec![0; 0x4000];
    image[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    image[16..18].copy_from_slice(&2_u16.to_le_bytes());
    image[18..20].copy_from_slice(&62_u16.to_le_bytes());
    image[20..24].copy_from_slice(&1_u32.to_le_bytes());
    image[24..32].copy_from_slice(&entry.to_le_bytes());
    image[32..40].copy_from_slice(&64_u64.to_le_bytes());
    image[52..54].copy_from_slice(&64_u16.to_le_bytes());
    image[54..56].copy_from_slice(&56_u16.to_le_bytes());
    image[56..58].copy_from_slice(&u16::try_from(segments.len())?.to_le_bytes());
    for (index, &[address, offset, file_size, memory_size, flags, alignment]) in
        segments.iter().enumerate()
    {
        let start = 64 + index * 56;
        image[start..start + 4].copy_from_slice(&1_u32.to_le_bytes());
        image[start + 4..start + 8].copy_from_slice(&u32::try_from(flags)?.to_le_bytes());
        for (offset, value) in [
            (8, offset),
            (16, address),
            (32, file_size),
            (40, memory_size),
            (48, alignment),
        ] {
            image[start + offset..start + offset + 8].copy_from_slice(&value.to_le_bytes());
        }
    }
    Ok(image)
}

#[test]
fn sectionless_image_preserves_file_bytes_and_zeroes_shared_page_gaps()
-> Result<(), Box<dyn std::error::Error>> {
    // Non-overlapping segments share the same RX page, with a BSS tail and padding.
    let mut bytes = image(
        0x1001,
        &[
            [0x1001, 0x1001, 1, 8, 5, 1],
            [0x1010, 0x2010, 2, 0x20, 5, 0],
        ],
    )?;
    bytes[0x1001..0x2000].fill(0xcc);
    bytes[0x2010..0x2020].fill(0x42);
    let plan = LoadPlan::from_elf(&bytes, Target::X86_64, 4096)?;
    let regions = plan.memory().regions();
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].range().start().get(), 0x1000);
    assert_eq!(regions[0].range().length(), 4096);
    let mut expected = [0; 32];
    expected[1] = 0xcc;
    expected[16..18].fill(0x42);
    let machine = Machine::from_elf(&bytes, Target::X86_64)?;
    assert_eq!(machine.read_memory(Address::new(0x1000), 32)?, expected);
    assert_eq!(machine.read_memory(Address::new(0x1012), 0x40)?, [0; 0x40]);
    assert!(machine.read_memory(Address::new(0x2000), 1).is_err());
    Ok(())
}

#[test]
fn permission_conflicts_and_runtime_requirements_are_explicit()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = image(
        0x1000,
        &[
            [0x1000, 0x1000, 4, 4, 5, 4096],
            [0x1004, 0x2004, 4, 4, 6, 4096],
        ],
    )?;
    assert_eq!(
        LoadPlan::from_elf(&bytes, Target::X86_64, 4096),
        Err(LoadError::PagePermissions)
    );
    // Dynamic loading, interpreters, TLS, RELRO, and GNU properties require policies.
    for kind in [2_u32, 3, 7, 0x6474_e552, 0x6474_e553] {
        let mut bytes = image(0x1000, &[[0x1000, 0x1000, 1, 1, 5, 4096]; 2])?;
        bytes[120..124].copy_from_slice(&kind.to_le_bytes());
        assert_eq!(
            LoadPlan::from_elf(&bytes, Target::X86_64, 4096),
            Err(LoadError::Runtime)
        );
    }
    Ok(())
}

#[test]
fn malformed_geometry_and_entry_padding_never_allocate_a_machine()
-> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        ([0x1000, 0x1000, 8, 4, 5, 4096], LoadError::Geometry),
        ([0x1000, 0x4000, 1, 1, 5, 4096], LoadError::Geometry),
        ([0x1000, 0x1001, 1, 1, 5, 4096], LoadError::Geometry),
        ([0x1000, 0x1000, 1, 1, 5, 3], LoadError::Geometry),
        (
            [0x1000, 0x1000, 0, MAX_MAPPED_BYTES + 1, 5, 4096],
            LoadError::Mapping(ValidationError::Length),
        ),
        ([0x1001, 0x1001, 1, 1, 5, 4096], LoadError::Entry),
        ([0x1000, 0x1000, 1, 1, 6, 4096], LoadError::Entry),
    ];
    for (segment, error) in cases {
        assert_eq!(
            LoadPlan::from_elf(&image(0x1000, &[segment])?, Target::X86_64, 4096),
            Err(error)
        );
    }
    let bytes = image(0x1000, &[[0x1000, 0x1000, 1, 1, 5, 4096]])?;
    assert_eq!(
        LoadPlan::from_elf(&bytes, Target::Aarch64, 4096),
        Err(LoadError::Target)
    );
    Ok(())
}

#[test]
fn final_address_page_and_fileless_storage_preserve_wide_geometry()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = image(u64::MAX, &[[u64::MAX, u64::MAX, 0, 1, 5, 4096]])?;
    let plan = LoadPlan::from_elf(&bytes, Target::X86_64, 4096)?;
    let region = &plan.memory().regions()[0];
    assert_eq!(region.range().end(), 1_u128 << 64);
    assert_eq!(region.range().length(), 4096);
    let machine = Machine::from_elf(&bytes, Target::X86_64)?;
    assert_eq!(machine.read_memory(Address::new(u64::MAX), 1)?, [0]);
    let bytes = image(u64::MAX, &[[u64::MAX, u64::MAX, 0, 2, 5, 4096]])?;
    assert_eq!(
        LoadPlan::from_elf(&bytes, Target::X86_64, 4096),
        Err(LoadError::Mapping(ValidationError::AddressOverflow))
    );
    Ok(())
}

#[test]
fn both_linked_targets_load_exact_data_and_bss_with_native_page_permissions()
-> Result<(), Box<dyn std::error::Error>> {
    for target in [Target::X86_64, Target::Aarch64] {
        let source = "nop\n.data\n.quad 42\n.bss\n.skip 32";
        let object = assembly::compile(target, source).map_err(|error| format!("{error:?}"))?;
        for base in [0, 0x1000, 0x1000 + target.instruction_alignment()] {
            let bytes = assembly::link(&object, Address::new(base))
                .map_err(|error| format!("{error:?}"))?;
            let elf = object::File::parse(bytes.as_slice())?;
            let machine = Machine::from_elf(&bytes, target)?;
            assert_eq!(machine.initial().entry().get(), base);
            assert_eq!(machine.initial().target(), target);
            let data = elf.section_by_name(".data").ok_or("missing data")?;
            let bss = elf.section_by_name(".bss").ok_or("missing bss")?;
            assert_eq!(
                machine.read_memory(Address::new(data.address()), 8)?,
                42_u64.to_le_bytes()
            );
            assert_eq!(
                machine.read_memory(Address::new(bss.address()), 32)?,
                [0; 32]
            );
        }
    }
    Ok(())
}
