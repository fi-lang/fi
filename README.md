# Shade Programming Language

Shade is a somewhat functional programming language with syntax and a type system inspired by [Purescript](https://www.purescript.org). The architecture of this compiler is heavily inspired by [rust-analyzer](https://www.github.com/rust-analyzer/rust-analyzer) and [Mun](https://www.github.com/mun-lang/mun). Currently the compiler uses [Cranelift](https://www.github.com/bytecodealliance/wasmtime) as the code generation backend. A new backend is being written in [lowlang](https://www.github.com/Xiulf/lowlang), which is heavily inspired by [SIL](https://github.com/apple/swift/blob/main/docs/SIL.rst) and uses [llvm](https://llvm.org) for compilation.
