fn main() {
    // Функции OpenGL резолвятся через get_proc_address, но символы линкует линкер.
    println!("cargo:rustc-link-lib=opengl32");
    // GLFW (статическая библиотека) использует Win32 API, которые линкер
    // сам не добавляет.
    for l in ["user32", "gdi32", "shell32", "ole32", "advapi32", "comdlg32", "winmm", "imm32"] {
        println!("cargo:rustc-link-lib=dylib={}", l);
    }
    println!("cargo:rerun-if-changed=build.rs");
}
