use io::Write;
use std::{io, process::Command};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;

use clang::{Clang, EntityKind, Index};

fn main() {
    const GENERATED_CODE_MARKER: &'static str = "\n\n/// Generated Code\n\n";
    println!("cargo:rerun-if-changed=cmake_depend/CMakeLists.txt");
    println!("cargo:rerun-if-changed=cmake_pico/CMakeLists.txt");
    println!("cargo:rerun-if-changed=cmake_pico/entry.c");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let target_triple = std::env::var("TARGET").unwrap();
    let mut gcc_command = if target_triple == "thumbv6m-none-eabi" {
        Command::new("arm-none-eabi-gcc")
    } else {
        Command::new("gcc")
    };
    let mut child = gcc_command
        .args(&["-xc", "-v", "-E", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute gcc command");

    let gcc_output = {
        let out = child.stderr.as_mut().expect("failed to open stderr of gcc");
        let mut gcc_output = String::new();
        out.read_to_string(&mut gcc_output)
            .expect("failed to read stderr of gcc");
        gcc_output
    };
    let mut gcc_output = gcc_output.split("\n");
    while let Some(s) = gcc_output.next() {
        if s.trim() == "#include <...> search starts here:" {
            break;
        }
    }

    let mut implicit_include_directories = Vec::new();
    while let Some(s) = gcc_output.next() {
        let s = s.trim();
        if Path::new(s).exists() {
            implicit_include_directories.push(s.to_string());
        } else {
            break;
        }
    }

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);

    let mut entry = {
        let entry_c = std::env::var("PICO_SDK_RS_CUSTOM_ENTRY_POINT")
            .map(|entry_point|
                match fs::read_to_string(&entry_point) {
                    Ok(mut s) => {
                        println!("cargo:rerun-if-changed={}", entry_point);
                        s.drain(s.find(GENERATED_CODE_MARKER).unwrap_or(s.len())..);
                        Ok(s)
                    }
                    e => e
                })
            .unwrap_or_else(|_| fs::read_to_string("cmake_pico/entry.c"))
            .expect("failed to read entry.c");

        let mut entry = File::create(out_dir.join("entry.c")).expect("failed to create entry.c");
        entry.write_all(entry_c.as_bytes())
            .expect("failed to write to entry.c");
        entry
    };

    cmake::Config::new("cmake_pico")
        .define("ENTRY_POINT", out_dir.join("entry.c"))
        // .build_target("all")
        .no_build_target(true)
        .build();

    fs::create_dir_all(out_dir.join("test")).expect("failed create_dir_all");
    std::env::set_var("OUT_DIR", out_dir.join("test").display().to_string());
    cmake::Config::new("cmake_depend")
        .target(guess_host_triple::guess_host_triple().unwrap_or("x86_64-unknown-linux-gnu"))
        .define(
            "DEPENDINFO_PATH",
            Path::new(&out_dir).join("build/CMakeFiles/pico.dir/DependInfo.cmake"),
        )
        .define(
            "INCLUDE_PATH_FILE",
            Path::new(&out_dir).join("include_path"),
        )
        .define("DEFINITIONS_FILE", Path::new(&out_dir).join("definitions"))
        .build_target("write")
        .build();
    std::env::set_var("OUT_DIR", out_dir.display().to_string());

    let clang = Clang::new().expect("failed Clang::new()");
    let index = Index::new(&clang, false, true);
    let mut parser = index.parser(out_dir.join("entry.c"));

    let include_directories = fs::read_to_string(out_dir.join("include_path"))
        .expect("failed to read include_path")
        .split(':')
        .map(|path| {
            out_dir.join("build").join(path.trim())
                .display().to_string()
        })
        .collect::<Vec<_>>();
    let clang_arguments = include_directories
        .iter()
        .map(|path| format!("-I{}", path))
        .chain(
            fs::read_to_string(out_dir.join("definitions")).expect("failed to read definitions")
                .split(':').map(|def| format!("-D{}", def)),
        )
        .chain(
            implicit_include_directories.iter()
                .map(|path| format!("-I{}", path)),
        )
        .collect::<Vec<_>>();
    parser.arguments(&clang_arguments);

    parser.skip_function_bodies(true);
    let parsed = parser.parse().expect("failed to parse");

    let mut code = String::from(GENERATED_CODE_MARKER);
    for entity in parsed.get_entity().get_children() {
        let location = entity.get_location().unwrap();
        let (location, _, _) = location.get_presumed_location();
        let location = location.into_bytes();
        if include_directories.iter().all(|dir| {
            let dir = dir.as_bytes();
            dir.len() > location.len() || &location[..dir.len()] != dir
        }) {
            println!("ignored: {:?}", entity);
            continue;
        }
        if entity.get_kind() == EntityKind::FunctionDecl {
            code += &format!(
                "{} wrapped_{}({}) {{ {}{1}({}); }}\n",
                entity.get_result_type().unwrap().get_display_name(),
                entity.get_name().unwrap(),
                {
                    let mut iter = entity.get_children().into_iter().filter_map(|entity| {
                        if entity.get_kind() == EntityKind::ParmDecl {
                            Some(format!(
                                "{} {}",
                                entity.get_type().unwrap().get_display_name(),
                                entity.get_name().unwrap()
                            ))
                        } else {
                            None
                        }
                    });
                    if let Some(mut current) = iter.next() {
                        while let Some(s) = iter.next() {
                            current.push_str(", ");
                            current.push_str(&s);
                        }
                        current
                    } else {
                        "".to_owned()
                    }
                },
                if entity.get_result_type().unwrap().get_display_name() == "void" {
                    ""
                } else {
                    "return "
                },
                {
                    let mut iter = entity.get_children().into_iter().filter_map(|entity| {
                        if entity.get_kind() == EntityKind::ParmDecl {
                            Some(entity.get_name().unwrap())
                        } else {
                            None
                        }
                    });
                    if let Some(mut current) = iter.next() {
                        while let Some(s) = iter.next() {
                            current.push_str(", ");
                            current.push_str(&s);
                        }
                        current
                    } else {
                        "".to_owned()
                    }
                }
            );
        }
    }

    entry.write_all(code.as_bytes())
        .expect("failed to write to entry.c");
    for alternative_path in std::env::var("PICO_SDK_RS_C_BINDING_ALTERNATIVES").unwrap_or(String::new())
        .trim()
        .split(":") {
        let mut file = match File::open(&alternative_path) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("failed to open file {} by {}", alternative_path, e);
                continue;
            }
        };
        let mut content = String::new();
        if let Err(e) = file.read_to_string(&mut content) {
            eprintln!("failed to read content by {}", e);
            continue;
        }
        let len = content.find(GENERATED_CODE_MARKER).unwrap_or(content.len());
        content.drain(len..);
        content.push_str(&code);
        if let Err(e) = fs::write(&alternative_path, content) {
            eprintln!("failed to write by {}", e);
            continue;
        }
    }

    let bindings = bindgen::builder()
        .header(out_dir.join("entry.c").display().to_string())
        .use_core()
        .ctypes_prefix("cty")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .detect_include_paths(true)
        .clang_args(clang_arguments)
        .clang_args(implicit_include_directories.iter().map(|path| format!("-I{}", path)))
        .clang_arg(format!("--target={}", target_triple))
        .whitelist_function("wrapped_.*")
        .generate()
        .expect("failed to generate binding");
    bindings.write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write bindings.rs");
}
