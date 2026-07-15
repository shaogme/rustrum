# Coding Guidelines and Instructions for Agents

When making modifications to this repository, please adhere to the following strict requirements. Failure to run and pass these checks will result in a Continuous Integration (CI) failure.

**IMPORTANT:** Always use Simplified Chinese (简体中文) when communicating and providing explanations.

## 核心原则 (Core Principles)

1. **回复语言**：始终使用**中文**回复。
2. **代码风格**：
   - **严禁使用 `mod.rs`**。必须遵守 Rust 2018 Edition 及更新版本的目录结构标准。
   - 模块 `foo` 应定义在 `foo.rs` 中；若有子模块，创建 `foo/` 目录，但父模块代码仍保留在 `foo.rs`，而非 `foo/mod.rs`。
3. **禁止猜测**：严禁猜测代码逻辑或文件内容；修改或回答前必须先读取相关代码。
4. **主动报告**：阅读代码时应主动报告潜在错误、安全漏洞、性能问题。
5. **绝对路径**：使用文件修改工具时（如 `write_to_file`、`replace_file_content`），**必须**使用**绝对路径**。
6. **Markdown 相对路径**：禁止在 Markdown (`*.md`) 文件中使用绝对路径链接（如 `file:///` 格式），必须使用相对路径链接。
7. **禁止使用 Emoji**：禁止在所有 Markdown (`*.md`) 文件中使用任何 Emoji 表情。

## 代码质量要求

- **质量与测试**: 注重代码质量、可测试性和测试覆盖。
- **编码规范**:
    - **禁止长路径**: 禁止在代码中使用全限定命名空间（尤其是以 `crate::` 开头的路径）超过 15 个字符。必须通过 `use` 语句导入后再调用。
    - **合并相同前缀的use语句**: 当有多个`use`语句具有相同前缀时，应合并为一条`use`语句，例如：
    ```rust
    //Bad
    use crate::nix::build;
    use crate::nix::store;
    use crate::nix::path;
    use crate::nix::refpath;
    //Good
    use crate::nix::{
        build,
        store,
        path,
        refpath,
    };
    ```
    - **cfg属性分组与换行隔离**: 所有 `#[cfg(...)]` 内的条件相同的use语句必须放在一起，并且与其他不同条件或不带 `cfg` 的use语句显式使用空行分隔。
    - **禁止在use嵌套导入内使用cfg**: 禁止在 `use {...}` 的 `{...}` 内部使用 `#[cfg(...)]`。

