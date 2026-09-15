//! clap owns argument relationships; ELF symbols retain their standard meaning.

use super::{
    super::{Guest, read_input},
    setup::Setup,
};
use object::{Object, ObjectSymbol, SymbolKind, SymbolSection};
use oplab_core::{
    address::Address,
    protocol::{Diagnostic, DiagnosticCode, MAX_OBJECT_BYTES, MAX_SOURCE_BYTES},
    target::Target,
};
use oplab_engine::{assembly, load::Image, machine::Machine, session::Session};

#[derive(clap::Subcommand)]
pub(crate) enum Input {
    /// Compile unchanged UTF-8 source from stdin, then load and run the linked ELF.
    Source {
        #[command(flatten)]
        input: super::super::Input,
        #[command(flatten)]
        policy: Policy,
        #[command(flatten)]
        setup: Setup,
    },
    /// Load and run a static ELF64 executable from stdin at its declared addresses.
    Elf {
        /// Required guest instruction set, checked against the ELF header.
        #[arg(value_enum)]
        target: Guest,
        #[command(flatten)]
        policy: Policy,
        #[command(flatten)]
        setup: Setup,
    },
    /// Map exact raw machine code from stdin read/execute, with no implicit ABI.
    Raw {
        #[command(flatten)]
        input: super::super::Input,
        /// Initial hexadecimal instruction address; defaults to the raw base.
        #[arg(long)]
        entry: Option<Address>,
        #[command(flatten)]
        policy: Policy,
        #[command(flatten)]
        setup: Setup,
    },
}

#[derive(clap::Args)]
#[command(group(clap::ArgGroup::new("completion").required(true).args(["until", "until_symbol"])))]
pub(crate) struct Policy {
    /// Stop successfully before fetching this hexadecimal guest address.
    #[arg(long)]
    until: Option<Address>,
    /// Stop before a uniquely named ELF address symbol.
    #[arg(long, value_name = "NAME")]
    until_symbol: Option<String>,
    /// Maximum instruction starts, including faulting instructions (REP counts once).
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..=100_000_000))]
    pub budget: u64,
    /// Execution timeout in milliseconds, checked between native slices.
    ///
    /// Starts after loading and observation validation. Native calls may overrun it.
    #[arg(long, default_value_t = 10_000, value_parser = clap::value_parser!(u64).range(1..=3_600_000))]
    pub timeout_ms: u64,
    /// Include final memory at this hexadecimal address (one mapped region).
    #[arg(long)]
    pub memory: Option<Address>,
    /// Final memory window length, from 1 to 65536 bytes; requires --memory.
    #[arg(long, requires = "memory", default_value_t = 64, value_parser = clap::value_parser!(u32).range(1..=65_536))]
    pub memory_bytes: u32,
}

impl Input {
    pub(super) fn prepare(self) -> Result<Prepared, Diagnostic> {
        match self {
            Self::Source {
                input,
                policy,
                setup,
            } => {
                let source = read_input(MAX_SOURCE_BYTES)?;
                let source = std::str::from_utf8(&source)
                    .map_err(|_| Diagnostic::new(DiagnosticCode::InvalidInput))?;
                let target = input.target.into();
                let object = assembly::compile(target, source)?;
                let image = assembly::link(&object, input.base)?;
                Prepared::new(Image::Elf(&image), target, setup, policy)
            }
            Self::Elf {
                target,
                policy,
                setup,
            } => {
                let image = read_input(MAX_OBJECT_BYTES)?;
                Prepared::new(Image::Elf(&image), target.into(), setup, policy)
            }
            Self::Raw {
                input,
                entry,
                policy,
                setup,
            } => {
                let bytes = read_input(MAX_OBJECT_BYTES)?;
                Prepared::new(
                    Image::Raw {
                        bytes: &bytes,
                        base: input.base,
                        entry: entry.unwrap_or(input.base),
                    },
                    input.target.into(),
                    setup,
                    policy,
                )
            }
        }
    }
}

impl Policy {
    fn completion(&self, image: Image<'_>) -> Result<Address, Diagnostic> {
        if let Some(address) = self.until {
            return Ok(address);
        }
        let invalid = || Diagnostic::new(DiagnosticCode::InvalidInput);
        let Image::Elf(image) = image else {
            return Err(invalid());
        };
        let name = self.until_symbol.as_deref().ok_or_else(invalid)?;
        let file = object::File::parse(image).map_err(|_| invalid())?;
        let mut matches = file.symbols().filter(|symbol| symbol.name() == Ok(name));
        let symbol = matches.next().ok_or_else(invalid)?;
        if matches.next().is_some() || matches!(symbol.kind(), SymbolKind::File | SymbolKind::Tls) {
            return Err(invalid());
        }
        match symbol.section() {
            SymbolSection::Absolute => {}
            SymbolSection::Section(index) => {
                file.section_by_index(index).map_err(|_| invalid())?;
            }
            _ => return Err(invalid()),
        }
        Ok(Address::new(symbol.address()))
    }
}

pub(super) struct Prepared {
    pub session: Session,
    pub target: Target,
    pub completion: Address,
    pub policy: Policy,
}

impl Prepared {
    fn new(
        image: Image<'_>,
        target: Target,
        setup: Setup,
        policy: Policy,
    ) -> Result<Self, Diagnostic> {
        let completion = policy.completion(image)?;
        let setup = setup.build(target)?;
        let machine =
            Machine::load(image, target, setup).map_err(|error| super::diagnostic(&error))?;
        let session = Session::new(machine, completion, policy.budget)
            .map_err(|error| super::diagnostic(&error))?;
        Ok(Self {
            session,
            target,
            completion,
            policy,
        })
    }
}
