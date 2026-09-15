use super::{FileFormat, read, write};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn selected_files_preserve_arbitrary_bytes_and_unicode(
        binary in proptest::collection::vec(any::<u8>(), 1..4096),
        text in proptest::collection::vec(any::<char>(), 0..64),
    ) {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("selected");
        let source = format!("\u{feff}{}\r\n", text.into_iter().collect::<String>());
        for (format, contents) in [(FileFormat::Binary, binary.as_slice()), (FileFormat::Source, source.as_bytes())] {
            // Standard filesystem I/O is the oracle in each direction.
            std::fs::write(&path, contents)?;
            prop_assert_eq!(read(&path, format), Ok(contents.to_vec()));
            std::fs::write(&path, b"previous contents")?;
            prop_assert_eq!(write(&path, contents, format), Ok(()));
            prop_assert_eq!(std::fs::read(&path)?, contents);
        }
    }
}

#[test]
fn file_budgets_accept_the_documented_boundary_and_reject_one_extra_byte()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("selected");
    // Independent product budgets: do not derive expected policy from limit().
    for (format, maximum) in [
        (FileFormat::Source, 256 * 1024),
        (FileFormat::Binary, 1024 * 1024),
        (FileFormat::Object, 1024 * 1024),
        (FileFormat::Image, 1024 * 1024),
    ] {
        let contents = vec![0x42; maximum];
        write(&path, &contents, format)?;
        assert_eq!(std::fs::read(&path)?, contents);
        assert_eq!(read(&path, format)?, contents);
        let oversized = vec![0x42; maximum + 1];
        assert_eq!(write(&path, &oversized, format), Err("file_size"));
        assert_eq!(std::fs::read(&path)?, contents);
        std::fs::write(&path, oversized)?;
        assert_eq!(read(&path, format), Err("file_size"));
    }
    Ok(())
}

#[test]
fn rejected_contents_and_paths_preserve_existing_files() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("selected");
    std::fs::write(&path, b"original")?;
    for (bytes, format, error) in [
        (&[0xff][..], FileFormat::Source, "file_encoding"),
        (&[][..], FileFormat::Binary, "file_size"),
        (&[][..], FileFormat::Object, "file_size"),
        (&[][..], FileFormat::Image, "file_size"),
    ] {
        assert_eq!(write(&path, bytes, format), Err(error));
        assert_eq!(std::fs::read(&path)?, b"original");
    }
    assert_eq!(
        read(&directory.path().join("missing"), FileFormat::Source),
        Err("file_read")
    );
    assert_eq!(read(directory.path(), FileFormat::Source), Err("file_read"));
    assert_eq!(
        write(directory.path(), b"nop", FileFormat::Source),
        Err("file_write")
    );
    assert!(directory.path().is_dir());
    std::fs::write(&path, [0xff])?;
    assert_eq!(read(&path, FileFormat::Source), Err("file_encoding"));
    write(&path, &[], FileFormat::Source)?;
    assert!(std::fs::read(&path)?.is_empty());
    assert!(read(&path, FileFormat::Source)?.is_empty());
    assert_eq!(read(&path, FileFormat::Binary), Err("file_size"));
    Ok(())
}
