use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    C,
    Cpp,
    Rust,
    Python,
    Java,
    JavaScript,
    TypeScript,
    Assembly,
    Go,
    Json,
    Toml,
    Yaml,
    Markdown,
    Shell,
    Html,
    Css,
}

impl Language {
    pub fn name(&self) -> &'static str {
        match self {
            Language::C => "C",
            Language::Cpp => "C++",
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::Java => "Java",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::Assembly => "Assembly",
            Language::Go => "Go",
            Language::Json => "JSON",
            Language::Toml => "TOML",
            Language::Yaml => "YAML",
            Language::Markdown => "Markdown",
            Language::Shell => "Shell",
            Language::Html => "HTML",
            Language::Css => "CSS",
        }
    }

    pub fn all() -> &'static [Language] {
        &[
            Language::C,
            Language::Cpp,
            Language::Rust,
            Language::Python,
            Language::Java,
            Language::JavaScript,
            Language::TypeScript,
            Language::Assembly,
            Language::Go,
            Language::Json,
            Language::Toml,
            Language::Yaml,
            Language::Markdown,
            Language::Shell,
            Language::Html,
            Language::Css,
        ]
    }

    pub fn from_id(id: &str) -> Option<Self> {
        let id_lower = id.trim().to_ascii_lowercase();
        match id_lower.as_str() {
            "c" => Some(Language::C),
            "cpp" | "c++" | "cuda" | "cu" => Some(Language::Cpp),
            "rust" | "rs" => Some(Language::Rust),
            "python" | "py" => Some(Language::Python),
            "java" => Some(Language::Java),
            "javascript" | "js" | "mjs" | "cjs" => Some(Language::JavaScript),
            "typescript" | "ts" => Some(Language::TypeScript),
            "assembly" | "asm" | "s" => Some(Language::Assembly),
            "go" => Some(Language::Go),
            "json" => Some(Language::Json),
            "toml" => Some(Language::Toml),
            "yaml" | "yml" => Some(Language::Yaml),
            "markdown" | "md" => Some(Language::Markdown),
            "shell" | "bash" | "sh" | "zsh" => Some(Language::Shell),
            "html" | "htm" => Some(Language::Html),
            "css" => Some(Language::Css),
            _ => None,
        }
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> Option<Self> {
        let p = path.as_ref();
        if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
            match file_name.to_ascii_lowercase().as_str() {
                "cargo.toml" => return Some(Language::Toml),
                "makefile" | "gnumakefile" => return Some(Language::Assembly),
                "dockerfile" => return Some(Language::Shell),
                _ => {}
            }
        }
        let ext = p.extension()?.to_str()?;
        Self::from_extension(ext)
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "c" | "h" => Some(Language::C),
            "cpp" | "hpp" | "cc" | "cxx" | "hh" | "hxx" | "cu" | "cuh" => Some(Language::Cpp),
            "rs" => Some(Language::Rust),
            "py" | "pyw" => Some(Language::Python),
            "java" => Some(Language::Java),
            "js" | "jsx" | "mjs" | "cjs" => Some(Language::JavaScript),
            "ts" | "tsx" => Some(Language::TypeScript),
            "s" | "asm" => Some(Language::Assembly),
            "go" => Some(Language::Go),
            "json" => Some(Language::Json),
            "toml" => Some(Language::Toml),
            "yaml" | "yml" => Some(Language::Yaml),
            "md" | "markdown" => Some(Language::Markdown),
            "sh" | "bash" | "zsh" => Some(Language::Shell),
            "html" | "htm" => Some(Language::Html),
            "css" => Some(Language::Css),
            _ => None,
        }
    }

    pub(crate) fn tree_sitter_language(&self) -> Option<tree_sitter::Language> {
        match self {
            Language::C => Some(tree_sitter_c::LANGUAGE.into()),
            Language::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),
            Language::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
            Language::Python => Some(tree_sitter_python::LANGUAGE.into()),
            Language::Java => Some(tree_sitter_java::LANGUAGE.into()),
            Language::JavaScript | Language::TypeScript => {
                Some(tree_sitter_javascript::LANGUAGE.into())
            }
            _ => None,
        }
    }
}

pub(crate) fn is_assembly_register(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "rax"
            | "rbx"
            | "rcx"
            | "rdx"
            | "rsi"
            | "rdi"
            | "rbp"
            | "rsp"
            | "r8"
            | "r9"
            | "r10"
            | "r11"
            | "r12"
            | "r13"
            | "r14"
            | "r15"
            | "eax"
            | "ebx"
            | "ecx"
            | "edx"
            | "esi"
            | "edi"
            | "ebp"
            | "esp"
            | "ax"
            | "bx"
            | "cx"
            | "dx"
            | "sp"
            | "bp"
            | "si"
            | "di"
            | "al"
            | "bl"
            | "cl"
            | "dl"
            | "ah"
            | "bh"
            | "ch"
            | "dh"
    )
}

pub(crate) fn is_assembly_mnemonic(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "mov"
            | "push"
            | "pop"
            | "lea"
            | "add"
            | "sub"
            | "imul"
            | "idiv"
            | "mul"
            | "div"
            | "inc"
            | "dec"
            | "and"
            | "or"
            | "xor"
            | "not"
            | "shl"
            | "shr"
            | "sal"
            | "sar"
            | "cmp"
            | "test"
            | "jmp"
            | "je"
            | "jne"
            | "jz"
            | "jnz"
            | "jg"
            | "jge"
            | "jl"
            | "jle"
            | "call"
            | "ret"
            | "leave"
            | "syscall"
            | "int"
            | "nop"
            | "hlt"
    )
}
