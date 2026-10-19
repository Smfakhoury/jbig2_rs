//! Internal conformance-vector decoder output helper.

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use jbig2_rs::Decoder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let input = PathBuf::from(arguments.next().ok_or("missing input path")?);
    let output = PathBuf::from(arguments.next().ok_or("missing output directory")?);
    if arguments.next().is_some() {
        return Err("unexpected extra argument".into());
    }

    let encoded = fs::read(&input)?;
    let document = Decoder::default().decode(&encoded)?;
    fs::create_dir_all(&output)?;
    for page in document.pages() {
        let bitmap = page.bitmap();
        let path = output.join(format!("page-{:08}.raw", page.number()));
        let mut file = fs::File::create(path)?;
        file.write_all(b"JB2R")?;
        file.write_all(&page.number().to_be_bytes())?;
        file.write_all(&bitmap.width().to_be_bytes())?;
        file.write_all(&bitmap.height().to_be_bytes())?;
        file.write_all(
            &u32::try_from(bitmap.stride())
                .map_err(|_| "stride does not fit u32")?
                .to_be_bytes(),
        )?;
        file.write_all(bitmap.bytes())?;
    }
    Ok(())
}
