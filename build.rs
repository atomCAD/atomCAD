use std::{
    io,
    fs,
    env,
    path::{Path, PathBuf},
};
use shaderc;

fn visit_files(dir: &Path, f: &mut dyn FnMut(&Path) -> io::Result<()>) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                visit_files(&path, f)?;
            } else {
                f(&path)?
            }
        }
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let mut compiler = shaderc::Compiler::new()
        .expect("failed to initialize glsl compiler");

    let out_dir: PathBuf = [&env::var("OUT_DIR").unwrap(), "shaders"].iter().collect();

    if !out_dir.exists() {
        fs::create_dir(&out_dir)?;
    }

    visit_files(Path::new("src/shaders"), &mut |path| {
        let binary = compiler.compile_into_spirv(
            &fs::read_to_string(path)?,
            match path.extension().and_then(|s| s.to_str()).unwrap() {
                "vert" => shaderc::ShaderKind::Vertex,
                "frag" => shaderc::ShaderKind::Fragment,
                "comp" => shaderc::ShaderKind::Compute,
                _ => panic!("unknown shader kind: {}", path.display()),
            },
            path.file_name().and_then(|s| s.to_str()).unwrap(),
            "main",
            None,
        ).unwrap();

        let mut new_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        new_path.push(path.strip_prefix("src").unwrap());

        fs::write(&new_path, binary.as_binary_u8())?;

        Ok(())
    })?;

    // Set debug cfg
    if let Ok(profile) = env::var("PROFILE") {
        println!("cargo:rustc-cfg=build={:?}", profile);
    }

    Ok(())
}