//! Target architecture, OS, ABI, endianness, object format, and subtarget features (`ir::Target`).

/// All 25 hardware target families in the Obsidian compiler suite.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Arch {
    X86_64 = 0,
    X86_32,
    AArch64,
    SystemZ,
    M68k,
    Arm,
    Thumb2,
    PowerPC64,
    Sparc64,
    Riscv64,
    Riscv32,
    LoongArch64,
    Mips64,
    Lanai,
    Arc,
    CSky,
    AmdGpu,
    NvPtx64,
    SpirV,
    DirectX,
    Ve,
    Wasm32,
    Wasm64,
    Bpf,
    Hexagon,
    Avr,
    Msp430,
    XCore,
    Xtensa,
}

impl Arch {
    /// Returns the canonical `.obs` target architecture name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X86_64      => "x86_64",
            Self::X86_32      => "x86",
            Self::AArch64     => "aarch64",
            Self::SystemZ     => "systemz",
            Self::M68k        => "m68k",
            Self::Arm         => "arm",
            Self::Thumb2      => "thumb2",
            Self::PowerPC64   => "powerpc64",
            Self::Sparc64     => "sparc64",
            Self::Riscv64     => "riscv64",
            Self::Riscv32     => "riscv32",
            Self::LoongArch64 => "loongarch64",
            Self::Mips64      => "mips64",
            Self::Lanai       => "lanai",
            Self::Arc         => "arc",
            Self::CSky        => "csky",
            Self::AmdGpu      => "amdgpu",
            Self::NvPtx64     => "nvptx64",
            Self::SpirV       => "spirv",
            Self::DirectX     => "dxil",
            Self::Ve          => "ve",
            Self::Wasm32      => "wasm32",
            Self::Wasm64      => "wasm64",
            Self::Bpf         => "bpf",
            Self::Hexagon     => "hexagon",
            Self::Avr         => "avr",
            Self::Msp430      => "msp430",
            Self::XCore       => "xcore",
            Self::Xtensa      => "xtensa",
        }
    }

    /// Returns the hardware pointer width in bits (`16`, `32`, or `64`).
    #[inline]
    pub const fn default_ptr_bits(self) -> u8 {
        match self {
            Self::Avr | Self::Msp430 => 16,
            Self::X86_32
            | Self::M68k
            | Self::Arm
            | Self::Thumb2
            | Self::Riscv32
            | Self::Lanai
            | Self::Arc
            | Self::CSky
            | Self::DirectX
            | Self::Wasm32
            | Self::Hexagon
            | Self::XCore
            | Self::Xtensa => 32,
            _ => 64,
        }
    }

    /// Returns the native hardware endianness of this architecture.
    #[inline]
    pub const fn default_endian(self) -> Endian {
        match self {
            Self::SystemZ | Self::M68k | Self::Sparc64 | Self::Lanai => Endian::Big,
            _ => Endian::Little,
        }
    }
}

/// Target byte order (`little` or `big`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Endian {
    Little = 0,
    Big    = 1,
}

impl Endian {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Little => "little",
            Self::Big    => "big",
        }
    }
}

/// Target operating system.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Os {
    None    = 0,
    Linux   = 1,
    MacOs   = 2,
    Windows = 3,
    FreeBsd = 4,
    Ios     = 5,
    Android = 6,
    Wasi    = 7,
}

impl Os {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None    => "none",
            Self::Linux   => "linux",
            Self::MacOs   => "macos",
            Self::Windows => "windows",
            Self::FreeBsd => "freebsd",
            Self::Ios     => "ios",
            Self::Android => "android",
            Self::Wasi    => "wasi",
        }
    }
}

/// Target ABI / environment.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Abi {
    None   = 0,
    SysV64 = 1,
    Darwin = 2,
    Msvc   = 3,
    Gnu    = 4,
    Musl   = 5,
    Eabi   = 6,
}

impl Abi {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None   => "none",
            Self::SysV64 => "sysv64",
            Self::Darwin => "darwin",
            Self::Msvc   => "msvc",
            Self::Gnu    => "gnu",
            Self::Musl   => "musl",
            Self::Eabi   => "eabi",
        }
    }
}

/// Binary container / object format produced for the target.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ObjFormat {
    Elf   = 0,
    MachO = 1,
    Coff  = 2,
    Wasm  = 3,
    Raw   = 4,
}

impl ObjFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Elf   => "elf",
            Self::MachO => "macho",
            Self::Coff  => "coff",
            Self::Wasm  => "wasm",
            Self::Raw   => "raw",
        }
    }
}

/// Complete target specification attached to an [`ir::Module`](crate::Module).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Target {
    pub arch: Arch,
    pub os: Os,
    pub abi: Abi,
    pub endian: Endian,
    pub ptr_bits: u8,
    pub obj_format: ObjFormat,
    /// Architecture-specific subtarget feature bitmask (`+avx2`, `+bmi2`, `+lse`, `+sve2`, etc.).
    pub features: u64,
}

impl Target {
    #[inline]
    pub const fn new(arch: Arch, os: Os, abi: Abi, obj_format: ObjFormat) -> Self {
        Self {
            arch,
            os,
            abi,
            endian: arch.default_endian(),
            ptr_bits: arch.default_ptr_bits(),
            obj_format,
            features: 0,
        }
    }

    /// `target arch=x86_64, os=linux, abi=sysv64` (ELF64 Little-Endian).
    #[inline]
    pub const fn x86_64_linux() -> Self {
        Self::new(Arch::X86_64, Os::Linux, Abi::SysV64, ObjFormat::Elf)
    }

    /// `target arch=x86_64, os=macos, abi=darwin` (Mach-O 64 Little-Endian).
    #[inline]
    pub const fn x86_64_macos() -> Self {
        Self::new(Arch::X86_64, Os::MacOs, Abi::Darwin, ObjFormat::MachO)
    }

    /// `target arch=x86_64, os=windows, abi=msvc` (PE32+/COFF Little-Endian).
    #[inline]
    pub const fn x86_64_windows() -> Self {
        Self::new(Arch::X86_64, Os::Windows, Abi::Msvc, ObjFormat::Coff)
    }

    /// `target arch=aarch64, os=macos, abi=darwin` (Mach-O 64 ARM64).
    #[inline]
    pub const fn aarch64_macos() -> Self {
        Self::new(Arch::AArch64, Os::MacOs, Abi::Darwin, ObjFormat::MachO)
    }

    /// `target arch=aarch64, os=linux, abi=gnu` (ELF64 ARM64).
    #[inline]
    pub const fn aarch64_linux() -> Self {
        Self::new(Arch::AArch64, Os::Linux, Abi::Gnu, ObjFormat::Elf)
    }

    /// `target arch=systemz, os=linux, abi=gnu` (ELF64 Big-Endian `s390x`).
    #[inline]
    pub const fn systemz_linux() -> Self {
        Self::new(Arch::SystemZ, Os::Linux, Abi::Gnu, ObjFormat::Elf)
    }

    /// `target arch=m68k, os=none, abi=eabi` (ELF32 Big-Endian Motorola 680x0).
    #[inline]
    pub const fn m68k_bare() -> Self {
        Self::new(Arch::M68k, Os::None, Abi::Eabi, ObjFormat::Elf)
    }

    /// `target arch=riscv64, os=linux, abi=gnu` (ELF64 Little-Endian `riscv64`).
    #[inline]
    pub const fn riscv64_linux() -> Self {
        Self::new(Arch::Riscv64, Os::Linux, Abi::Gnu, ObjFormat::Elf)
    }

    /// `target arch=wasm32, os=wasi, abi=none` (WebAssembly `.wasm`).
    #[inline]
    pub const fn wasm32_wasi() -> Self {
        Self::new(Arch::Wasm32, Os::Wasi, Abi::None, ObjFormat::Wasm)
    }

    /// Returns a copy of `self` with the given hardware feature bitmask enabled.
    #[inline]
    pub const fn with_features(mut self, features: u64) -> Self {
        self.features |= features;
        self
    }

    /// Returns `true` if all bits in `feature_mask` are enabled on this target.
    #[inline]
    pub const fn has_feature(self, feature_mask: u64) -> bool {
        (self.features & feature_mask) == feature_mask
    }

    /// Returns the pointer size in bytes (`2`, `4`, or `8`).
    #[inline]
    pub const fn ptr_bytes(self) -> u8 {
        self.ptr_bits / 8
    }

    /// Returns `true` if the target uses Big-Endian byte ordering.
    #[inline]
    pub const fn is_big_endian(self) -> bool {
        matches!(self.endian, Endian::Big)
    }
}
