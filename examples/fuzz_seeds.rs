//! Generates project-owned fuzz seeds without official or third-party assets.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

#[path = "../tests/support/context_scenarios.rs"]
mod context_scenarios;
#[allow(dead_code)]
#[path = "../tools/distribution_consumer.rs"]
mod fixture;

fn write_seed(path: &Path, bytes: &[u8]) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || fs::read(path)? != bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid or changed generated seed: {}", path.display()),
                ));
            }
            return Ok(());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let output = arguments.next().ok_or("missing corpus directory")?;
    if arguments.next().is_some() {
        return Err("unexpected extra argument".into());
    }
    let (standalone, embedded) = fixture::generated_page();
    fixture::main()?;
    let zeroes = [0_u8; 32];
    let ones = [0xff_u8; 32];
    let seeds = [
        ("generated-v1-zeroes", zeroes.as_slice()),
        ("generated-v1-ones", ones.as_slice()),
        ("generated-v1-page", standalone.as_slice()),
        ("generated-v1-embedded", embedded.as_slice()),
    ];
    for target in [
        "file_header_arbitrary",
        "file_header_structured",
        "document_arbitrary",
        "arithmetic",
        "huffman",
        "mmr",
        "generic_region",
        "document_context_structured",
    ] {
        let directory = Path::new(&output).join(target);
        fs::create_dir_all(&directory)?;
        for (name, bytes) in seeds {
            write_seed(&directory.join(name), bytes)?;
        }
    }
    for (name, controls) in [
        ("generated-context-v1-white", [0_u8; 8]),
        (
            "generated-context-v1-transitions",
            [63, 15, 2, 1, 0, 0, 0, 0],
        ),
        ("generated-context-v1-recovery", [0, 0, 0, 0, 63, 15, 2, 1]),
    ] {
        let mut controls = controls.to_vec();
        for operation in [8, 9, 4, 6, 7, 1, 3, 2] {
            controls.extend_from_slice(&[operation, 0, 2]);
        }
        context_scenarios::exercise(&controls);
        write_seed(
            &Path::new(&output)
                .join("document_context_structured")
                .join(name),
            &controls,
        )?;
    }
    Ok(())
}
