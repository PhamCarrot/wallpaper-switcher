#[cfg(windows)]
fn main() {
    use std::ffi::CString;
    use std::fs;
    use std::path::PathBuf;
    use windows::core::PCSTR;
    use windows::Win32::Graphics::Direct3D::Fxc::{
        D3DCompile, D3DCOMPILE_ENABLE_STRICTNESS, D3DCOMPILE_OPTIMIZATION_LEVEL3,
    };
    use windows::Win32::Graphics::Direct3D::{ID3DBlob, ID3DInclude};

    #[cfg(target_env = "msvc")]
    {
        // The raised desktop needs a layered child window under the icon view.
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/app.manifest");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }

    let source_path = PathBuf::from("shaders/transition.hlsl");
    println!("cargo:rerun-if-changed={}", source_path.display());
    let source = fs::read(&source_path).expect("read transition shader source");
    let source_name = CString::new(source_path.to_string_lossy().as_bytes())
        .expect("transition shader path contains no NUL");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    embed_locales(&output);

    for (entry, target, name) in [
        ("VSMain", "vs_5_0", "transition.vs.cso"),
        ("PSMain", "ps_5_0", "transition.ps.cso"),
    ] {
        let entry = CString::new(entry).unwrap();
        let target = CString::new(target).unwrap();
        let mut code: Option<ID3DBlob> = None;
        let mut errors: Option<ID3DBlob> = None;
        let result = unsafe {
            D3DCompile(
                source.as_ptr().cast(),
                source.len(),
                PCSTR(source_name.as_ptr().cast()),
                None,
                None::<&ID3DInclude>,
                PCSTR(entry.as_ptr().cast()),
                PCSTR(target.as_ptr().cast()),
                D3DCOMPILE_ENABLE_STRICTNESS | D3DCOMPILE_OPTIMIZATION_LEVEL3,
                0,
                &mut code,
                Some(&mut errors),
            )
        };
        if let Err(error) = result {
            let details = errors
                .as_ref()
                .map(|blob| unsafe {
                    let bytes = std::slice::from_raw_parts(
                        blob.GetBufferPointer().cast::<u8>(),
                        blob.GetBufferSize(),
                    );
                    String::from_utf8_lossy(bytes).into_owned()
                })
                .unwrap_or_default();
            panic!("could not compile {name}: {error}\n{details}");
        }
        let code = code.expect("D3DCompile returned no shader bytecode");
        let bytes = unsafe {
            std::slice::from_raw_parts(code.GetBufferPointer().cast::<u8>(), code.GetBufferSize())
        };
        fs::write(output.join(name), bytes).expect("write compiled transition shader");
    }
}

#[cfg(windows)]
fn embed_locales(output: &std::path::Path) {
    use std::{fmt::Write, fs, path::PathBuf};
    fn resources(root: PathBuf, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(root).expect("read embedded locale directory") {
            let path = entry.expect("read embedded locale entry").path();
            if path.is_dir() {
                resources(path, files);
            } else if path.extension().is_some_and(|ext| ext == "ftl") {
                files.push(path);
            }
        }
    }
    let root = PathBuf::from("assets/locales");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut locales: Vec<_> = fs::read_dir(&root)
        .expect("read embedded locales")
        .map(|entry| entry.expect("read locale").path())
        .filter(|path| path.is_dir())
        .collect();
    locales.sort();
    let mut code = String::from("const LOCALE_RESOURCES: &[(&str, &[&str])] = &[\n");
    for locale in locales {
        let tag = locale
            .file_name()
            .unwrap()
            .to_str()
            .expect("UTF-8 locale tag");
        writeln!(code, "({tag:?}, &[").unwrap();
        let mut files = Vec::new();
        resources(locale.clone(), &mut files);
        files.sort();
        for path in files {
            let path = path.to_str().expect("UTF-8 locale path").replace('\\', "/");
            println!("cargo:rerun-if-changed={path}");
            writeln!(
                code,
                "include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{path}\")),"
            )
            .unwrap();
        }
        code.push_str("]),\n");
    }
    code.push_str("];\n");
    fs::write(output.join("embedded_locales.rs"), code).expect("write embedded locale bindings");
}

#[cfg(not(windows))]
fn main() {
    panic!("Wallpaper Switcher can only be built on Windows");
}
