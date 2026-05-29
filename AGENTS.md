# 🤖 Agent Developer Guidelines

This document outlines the development guidelines, architecture, and coding standards for contributing to the **Carrier** project. Please read this document and the referenced [README.md](./README.md) before implementing changes.

---

## 📖 Project Overview & Context

- For general project setup, CLI options, usage guides, and the ecosystem roadmap, see the [README.md](./README.md).
- **Core Goal**: Carrier is a planned universal package manager aiming to unify package operations under a single CLI interface, beginning with the `outdated` command.

---

## 🏛️ Architecture: Micro-Kernel Design

Carrier is designed using a **micro-kernel architecture**:
1. **The Kernel (Core)**: A thin driver located in `src/core/`. It handles command routing, ecosystem detection, orchestration, and output formatting. It is agnostic to the internal details of any specific package ecosystem.
2. **The Providers**: Located in `src/providers/`. Each package manager/ecosystem (NuGet, npm, Cargo, etc.) is implemented as a standalone module implementing the `Provider` trait defined in `src/core/provider.rs`.
3. **Pluggable Registration**: Providers are dynamically registered into the `ProviderRegistry` inside `src/providers/mod.rs`.

When adding or modifying features, respect this separation. The core should never have imports or conditional logic tied to specific provider details.

---

## 🛠️ Coding Standards & Principles

All code written for Carrier must adhere to the following standards:

### 1. Test-Driven Development (TDD)
- **Write Tests First/Alongside Code**: Implement tests to verify functionality before or concurrently with code changes.
- **Unit Testing**: Keep functions pure and modular so they can be easily tested. Do not rely on external networks in unit tests (e.g., mock or test parser inputs locally).
- **Integration Testing**: Use integration tests to verify the end-to-end flow where appropriate.

### 2. SOLID Principles
- **Single Responsibility (SRP)**: Keep modules focused. For example, keep registry network calls separate from manifest parsing logic (e.g., see the division in `providers/nuget/parser.rs` and `providers/nuget/registry.rs`).
- **Open-Closed Principle (OCP)**: The registry should be open for extension (adding new providers) but closed for modification. You should be able to support a new ecosystem simply by implementing `Provider` and adding one line to the registry function.
- **Liskov Substitution / Interface Segregation**: Program to the `Provider` trait boundary. Avoid downcasting or checking for concrete provider types.
- **Dependency Inversion**: High-level modules (the orchestrator) depend on abstractions (`Provider` trait), not concrete implementations.

### 3. DRY (Don't Repeat Yourself)
- Avoid duplicating parsing patterns, XML/JSON readers, version normalization logic, or error handling. Centralize shared utility code in `src/core/` when applicable.

### 4. Strictly Synchronous (No Async)
- Carrier does not use async Rust. Do not introduce `tokio`, `async-std`, or `async/await` syntax.
- Use `ureq` for HTTP requests, and the standard library (`std::fs`, `std::path`) for I/O operations.

---

## 💬 Communication & Decision-Making Guidelines

As an AI agent, you must execute tasks with high precision:
1. **Reduce Assumptions**: Do not guess or assume user requirements, preferred designs, or target endpoints.
2. **Ask for Clarification**: If you are not **100% sure** about any requirement, design choice, or code integration:
   - **Stop execution.**
   - **Ask the user directly** to clarify before writing code or making modifications.
3. **Preserve Documentation**: Do not remove or alter existing inline code documentation or comments unless directly instructed.
