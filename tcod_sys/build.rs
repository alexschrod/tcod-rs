extern crate cc;
extern crate pkg_config;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};


fn build_libz(libz_sources: &[&str]) {
    let mut config = cc::Build::new();
    for c_file in libz_sources {
        config.file(c_file);
    }
    config.flag("-w");
    config.compile("libz.a");
}

fn build_libtcod_objects(mut config: cc::Build, sources: &[&str]) {
    config.include("libtcod/include");
    config.include("libtcod/src/zlib");
    for c_file in sources {
        config.file(c_file);
    }
    config.cargo_metadata(false);
    config.flag("-w");
    config.compile("libtcod.a");
}


fn compile_config(config: cc::Build) {
    let mut cmd = config.get_compiler().to_command();
    println!("Compiling: {:?}", cmd);
    match cmd.output() {
        Ok(output) => {
            println!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
            println!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
            if !output.status.success() {
                panic!("Compilation failed.");
            }
        }
        Err(e) => {
            panic!("Failed to run the compilation command {}.", e);
        }
    }
}


/// Build static libtcod for Linux
#[cfg(not(feature = "dynlib"))]
fn build_linux_static(_dst: &Path, libtcod_sources: &[& 'static str]) {
    // Tell rust to link the produced library
    // It is important to specify this first, so that the library will be linked
    // before SDL2, as the link order matters for static libraries
    println!("cargo:rustc-link-lib=static=tcod");
    let mut config = cc::Build::new();
    // Add dependencies
    for include_path in &pkg_config::find_library("sdl2").unwrap().include_paths {
        config.include(include_path);
    }
    // Build the library
    config.define("TCOD_SDL2", None);
    config.define("NO_OPENGL", None);
    config.define("NDEBUG", None);
    config.flag("-fno-strict-aliasing");
    config.flag("-ansi");
    build_libtcod_objects(config, libtcod_sources);
}


/// Build dynamic libtcod for Linux
#[cfg(feature = "dynlib")]
fn build_linux_dynamic(dst: &Path, libtcod_sources: &[& 'static str]) {
    // Build the *.o files:
    {
        let mut config = cc::Build::new();
        for include_path in &pkg_config::find_library("sdl2").unwrap().include_paths {
            config.include(include_path);
        }
        config.define("TCOD_SDL2", None);
        config.define("NO_OPENGL", None);
        config.define("NDEBUG", None);
        config.flag("-fno-strict-aliasing");
        config.flag("-ansi");
        build_libtcod_objects(config, libtcod_sources);
    }

    // Build the DLL
    let mut config = cc::Build::new();
    config.define("TCOD_SDL2", None);
    config.define("NO_OPENGL", None);
    config.define("NDEBUG", None);
    config.flag("-shared");
    config.flag("-Wl,-soname,libtcod.so");
    config.flag("-o");
    config.flag(dst.join("libtcod.so").to_str().unwrap());
    for c_file in libtcod_sources {
        config.flag(dst.join(c_file).with_extension("o").to_str().unwrap());
    }
    config.flag(dst.join("libz.a").to_str().unwrap());
    config.flag("-lSDL2");
    config.flag("-lX11");
    config.flag("-lm");
    config.flag("-ldl");
    config.flag("-lpthread");

    compile_config(config);
    assert!(dst.join("libtcod.so").is_file());

    pkg_config::find_library("x11").unwrap();
}

#[cfg(feature = "generate_bindings")]
fn generate_bindings<P: AsRef<Path>>(dst_dir: P) {

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=wrapper.h");

    let bindings = bindgen::Builder::default()
        .header("bindgen.h")
        .emit_builtins()
        .default_enum_style(bindgen::EnumVariation::Rust{non_exhaustive:false})
        .derive_default(true)
        .bitfield_enum("TCOD_font_flags_t")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let bindings_file = dst_dir.as_ref().join("bindings.rs");
    bindings
        .write_to_file(&bindings_file)
        .expect("Couldn't write bindings!");

    // Copy bindings to $TARGET_bindings.rs
    let target = env::var("TARGET").unwrap();
    let target_bindings_file = format!("{}_bindings.rs", target);
    std::fs::copy(bindings_file, &target_bindings_file).unwrap();
    println!("cargo:rustc-env=BINDINGS_TARGET={}", target);

    // Tell cargo to invalidate the built crate whenever the bindings change
    println!("cargo:rerun-if-changed={}", target_bindings_file);
}

#[cfg(not(feature = "generate_bindings"))]
fn validate_bindings_for_target() {
    let target = env::var("TARGET").unwrap();
    let target_bindings_file = format!("{}_bindings.rs", target);
    let target_bindings_path: &Path = target_bindings_file.as_ref();
    if !target_bindings_path.exists() {
        panic!("No bindings found for {}", target);
    }
    println!("cargo:rustc-env=BINDINGS_TARGET={}", target);

    // Tell cargo to invalidate the built crate whenever the bindings change
    println!("cargo:rerun-if-changed={}", target_bindings_file);
}

fn main() {
    let is_crater = option_env!("CRATER_TASK_TYPE");

    if is_crater.is_some() {
        return;
    }

    let src_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let dst_dir = env::var("OUT_DIR").unwrap();
    let target = env::var("TARGET").unwrap();

    #[cfg(feature = "generate_bindings")]
    generate_bindings(&dst_dir);
    #[cfg(not(feature = "generate_bindings"))]
    validate_bindings_for_target();

    let src = Path::new(&src_dir);
    let dst = Path::new(&dst_dir);
    let sdl_lib_dir = src.join("libtcod").join("dependencies").join("SDL2-2.0.7").join("lib").join(&target);
    let sdl_include_dir = src.join("libtcod").join("dependencies").join("SDL2-2.0.7").join("include");

    let libz_sources = &[
        "libtcod/src/zlib/adler32.c",
	    "libtcod/src/zlib/crc32.c",
	    "libtcod/src/zlib/deflate.c",
	    "libtcod/src/zlib/infback.c",
	    "libtcod/src/zlib/inffast.c",
	    "libtcod/src/zlib/inflate.c",
	    "libtcod/src/zlib/inftrees.c",
	    "libtcod/src/zlib/trees.c",
	    "libtcod/src/zlib/zutil.c",
	    "libtcod/src/zlib/compress.c",
	    "libtcod/src/zlib/uncompr.c",
	    "libtcod/src/zlib/gzclose.c",
	    "libtcod/src/zlib/gzlib.c",
	    "libtcod/src/zlib/gzread.c",
	    "libtcod/src/zlib/gzwrite.c",
    ];

    let libtcod_sources = &[
 	    "libtcod/src/bresenham_c.c",
	    "libtcod/src/bsp_c.c",
	    "libtcod/src/color_c.c",
	    "libtcod/src/console_c.c",
        "libtcod/src/console_rexpaint.c",
	    "libtcod/src/fov_c.c",
	    "libtcod/src/fov_circular_raycasting.c",
	    "libtcod/src/fov_diamond_raycasting.c",
	    "libtcod/src/fov_permissive2.c",
	    "libtcod/src/fov_recursive_shadowcasting.c",
	    "libtcod/src/fov_restrictive.c",
	    "libtcod/src/heightmap_c.c",
	    "libtcod/src/image_c.c",
	    "libtcod/src/lex_c.c",
	    "libtcod/src/list_c.c",
	    "libtcod/src/mersenne_c.c",
	    "libtcod/src/namegen_c.c",
	    "libtcod/src/noise_c.c",
	    "libtcod/src/parser_c.c",
	    "libtcod/src/path_c.c",
	    "libtcod/src/sys_c.c",
	    "libtcod/src/sys_sdl2_c.c",
	    "libtcod/src/sys_sdl_c.c",
	    "libtcod/src/sys_sdl_img_bmp.c",
	    "libtcod/src/sys_sdl_img_png.c",
	    "libtcod/src/tree_c.c",
	    "libtcod/src/txtfield_c.c",
	    "libtcod/src/wrappers.c",
	    "libtcod/src/zip_c.c",
	    "libtcod/src/png/lodepng.c",
    ];

    if target.contains("linux") {
        build_libz(libz_sources);

        #[cfg(not(feature = "dynlib"))]
        build_linux_static(&dst, libtcod_sources);
        #[cfg(feature = "dynlib")]
        build_linux_dynamic(&dst, libtcod_sources);

    } else if target.contains("darwin") {
        build_libz(libz_sources);

        // Build the *.o files
        {
            let mut config = cc::Build::new();
            for include_path in &pkg_config::find_library("sdl2").unwrap().include_paths {
                config.include(include_path);
            }
            config.define("TCOD_SDL2", None);
            config.define("NO_OPENGL", None);
            config.define("NDEBUG", None);
            config.flag("-fno-strict-aliasing");
            config.flag("-ansi");
            build_libtcod_objects(config, libtcod_sources);
        }

        // Build the DLL
        let mut config = cc::Build::new();
        config.define("TCOD_SDL2", None);
        config.define("NO_OPENGL", None);
        config.define("NDEBUG", None);
        config.flag("-shared");
        config.flag("-o");
        config.flag(dst.join("libtcod.dylib").to_str().unwrap());
        for c_file in libtcod_sources {
            config.flag(dst.join(c_file).with_extension("o").to_str().unwrap());
        }
        config.flag(dst.join("libz.a").to_str().unwrap());
        config.flag("-lSDL2");
        config.flag("-lSDL2main");
        config.flag("-framework");
        config.flag("Cocoa");
        config.flag("-lm");
        config.flag("-ldl");
        config.flag("-lpthread");

        compile_config(config);
        assert!(dst.join("libtcod.dylib").is_file());

        println!("cargo:rustc-link-lib=framework=Cocoa");

    } else if target.contains("windows-gnu") {
        assert!(sdl_lib_dir.is_dir());
        assert!(sdl_include_dir.is_dir());
        fs::copy(&sdl_lib_dir.join("SDL2.dll"), &dst.join("SDL2.dll")).unwrap();
        fs::copy(&sdl_lib_dir.join("libSDL2.a"), &dst.join("libSDL2.a")).unwrap();
        fs::copy(&sdl_lib_dir.join("libSDL2main.a"), &dst.join("libSDL2main.a")).unwrap();


        // Build the DLL
        let mut config = cc::Build::new();
        config.flag("-fno-strict-aliasing");
        config.flag("-ansi");
        config.define("TCOD_SDL2", None);
        config.define("NO_OPENGL", None);
        config.define("NDEBUG", None);
        config.define("LIBTCOD_EXPORTS", None);
        config.flag("-o");
        config.flag(dst.join("libtcod.dll").to_str().unwrap());
        config.flag("-shared");
        fs::create_dir(dst.join("lib")).unwrap();
        config.flag(&format!("-Wl,--out-implib,{}", dst.join("lib/libtcod.a").display()));
        config.include(Path::new("libtcod").join("src").join("zlib"));
        config.include(Path::new("libtcod").join("include"));
        for c_file in libz_sources.iter().chain(libtcod_sources) {
            let path = c_file.split('/').fold(PathBuf::new(), |path, segment| path.join(segment));
            config.flag(src.join(path).to_str().unwrap());
        }
        config.flag("-mwindows");
        config.flag("-L");
        config.flag(sdl_lib_dir.to_str().unwrap());
        config.flag("-lSDL2");
        config.flag("-lSDL2main");
        config.flag(&format!("-I{}", sdl_include_dir.to_str().unwrap()));
        config.flag("-static-libgcc");
        config.flag("-static-libstdc++");

        compile_config(config);
        assert!(dst.join("libtcod.dll").is_file());

        println!("cargo:rustc-link-lib=dylib=SDL2");
        println!("cargo:rustc-link-search=native={}", sdl_lib_dir.display());
        println!("cargo:rustc-link-search=native={}", dst.display());

    } else if target.contains("windows-msvc") {
        assert!(sdl_lib_dir.is_dir());
        assert!(sdl_include_dir.is_dir());
        fs::copy(&sdl_lib_dir.join("SDL2.dll"), &dst.join("SDL2.dll")).unwrap();
        fs::copy(&sdl_lib_dir.join("SDL2.lib"), &dst.join("SDL2.lib")).unwrap();
        fs::copy(&sdl_lib_dir.join("SDL2main.lib"), &dst.join("SDL2main.lib")).unwrap();

        // Build the DLL
        let mut config = cc::Build::new();
        config.flag("/DTCOD_SDL2");
        config.flag("/DNO_OPENGL");
        config.flag("/DNDEBUG");
        config.flag("/DLIBTCOD_EXPORTS");
        config.flag(&format!("/Fo:{}\\", dst.to_str().unwrap()));
        config.include(sdl_include_dir.to_str().unwrap());
        config.include(Path::new("libtcod").join("src").join("zlib"));
        config.include(Path::new("libtcod").join("include"));
        for c_file in libz_sources.iter().chain(libtcod_sources) {
            // Make sure the path is in the Windows format. This
            // shouldn't matter but it's distracting when debugging
            // build script issues.
            let path = c_file.split('/').fold(PathBuf::new(), |path, segment| path.join(segment));
            config.flag(src.join(path).to_str().unwrap());
        }
        config.flag("User32.lib");
        config.flag("SDL2.lib");
        config.flag("SDL2main.lib");
        config.flag("/link");
        config.flag(&format!("/LIBPATH:{}", dst.to_str().unwrap()));
        config.flag("/DLL");
        config.flag(&format!("/OUT:{}", dst.join("tcod.dll").display()));

        compile_config(config);
        assert!(dst.join("tcod.dll").is_file());

        println!("cargo:rustc-link-search={}", dst.display());
        println!("cargo:rustc-link-lib=dylib=SDL2");
        println!("cargo:rustc-link-lib=User32");
    }

    println!("cargo:rustc-link-search={}", dst.display());
    println!("cargo:rustc-link-lib=dylib=tcod");
    println!("cargo:rustc-link-lib=SDL2");
}
