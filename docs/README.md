# Chopin Documentation

Complete documentation for the Chopin web framework.

## 📚 Table of Contents

### Getting Started

- **[Getting Started Guide](getting-started.md)** - Your first Chopin application
- **[CLI Reference](cli.md)** - Complete command-line tool documentation
- **[CLI Cheat Sheet](cli-cheatsheet.md)** - Quick reference for common tasks

### Core Concepts

- **[Architecture](architecture.md)** - Framework design and structure
- **[Configuration](configuration.md)** - Environment variables and settings
- **[API Reference](api.md)** - Request/response format and conventions

### Development Guides

- **[Models & Database](models-database.md)** - Working with SeaORM and migrations
- **[Controllers & Routing](controllers-routing.md)** - Creating endpoints and routes
- **[Testing](testing.md)** - Unit, integration, and API testing

### Advanced Features

- **[Caching](caching.md)** - Redis and in-memory caching layer
- **[File Uploads](file-uploads.md)** - Handle multipart uploads and storage
- **[Roles & Permissions](roles-permissions.md)** - Role-based access control (RBAC)
- **[GraphQL](graphql.md)** - GraphQL API support via async-graphql
- **[LLM Learning Guide](llm-learning-guide.md)** - Share with LLMs (ChatGPT, Claude) so they can help you build Chopin apps

### Contributing

- **[Contributing Guide](../CONTRIBUTING.md)** - How to contribute to Chopin

---

## 🚀 Quick Links

### New to Chopin?

1. Start with [Getting Started](getting-started.md)
2. Read the [Architecture](architecture.md) overview
3. Explore [CLI Cheat Sheet](cli-cheatsheet.md) for quick commands
4. Build something with [Controllers](controllers-routing.md)

### Asking LLMs for Help?

- [LLM Learning Guide](llm-learning-guide.md) - Copy/paste this to ChatGPT, Claude to help them assist you

### Building Your API?

- [Models & Database](models-database.md) - Define your data
- [Controllers & Routing](controllers-routing.md) - Create endpoints
- [API Reference](api.md) - Response formats
- [Testing](testing.md) - Write tests

### Going to Production?

- [Configuration](configuration.md) - Set up environment
- [Security](security.md) - Security checklist
- [Performance](performance.md) - Optimize performance
- [Deployment](deployment.md) - Deploy your app

---

## 📖 Documentation Guide

### By Task

| I want to... | Read this... |
|--------------|--------------|
| Create a new project | [Getting Started](getting-started.md) |
| Generate models | [CLI Reference](cli.md#chopin-generate-model) |
| Make database queries | [Models & Database](models-database.md#queries) |
| Create API endpoints | [Controllers & Routing](controllers-routing.md) |
| Add authentication | [API Reference](api.md#authentication) |
| Add role-based access | [Roles & Permissions](roles-permissions.md) |
| Cache data (Redis) | [Caching](caching.md) |
| Handle file uploads | [File Uploads](file-uploads.md) |
| Add GraphQL | [GraphQL](graphql.md) |
| Write tests | [Testing](testing.md) |
| Configure environment | [Configuration](configuration.md) |
| Optimize performance | [Performance](performance.md) |
| Deploy to production | [Deployment](deployment.md) |
| Secure my app | [Security](security.md) |
| Ask LLMs for help | [LLM Learning Guide](llm-learning-guide.md) |

### By Level

**Beginner** (new to Chopin):
1. [Getting Started](getting-started.md)
2. [CLI Cheat Sheet](cli-cheatsheet.md)
3. [Models & Database](models-database.md)
4. [Controllers & Routing](controllers-routing.md)

**Intermediate** (building production apps):
1. [Architecture](architecture.md)
2. [Testing](testing.md)
3. [Configuration](configuration.md)
4. [Roles & Permissions](roles-permissions.md)
5. [Caching](caching.md)
6. [File Uploads](file-uploads.md)

**Advanced** (optimizing and scaling):
1. [GraphQL](graphql.md)
2. [Security](security.md)
3. [Deployment](deployment.md)
4. [Performance](performance.md)
5. [Contributing](../CONTRIBUTING.md)

---

## 📝 Guide Summaries

### [Getting Started](getting-started.md)
Install Chopin, create your first project, generate models, and understand the basics. Perfect for new users.

**Topics**: Installation, Project creation, Model generation, Built-in auth, Testing

### [CLI Reference](cli.md)
Complete reference for the `chopin` command-line tool with detailed examples and use cases.

**Commands**: `new`, `generate model`, `generate controller`, `db migrate`, `db rollback`, `db status`, `db reset`, `db seed`, `docs export`, `run`, `createsuperuser`, `info`

### [CLI Cheat Sheet](cli-cheatsheet.md)
Quick reference card with common commands, field types, and workflows. Keep this handy!

**Includes**: Field types table, common patterns, quick commands

### [Architecture](architecture.md)
Understand Chopin's design, components, and how they fit together.

**Topics**: Framework stack, Request lifecycle, Data flow, Design decisions

### [Configuration](configuration.md)
Everything about environment variables, database URLs, JWT secrets, and configuration patterns.

**Topics**: Environment variables, Database config, JWT settings, Production setup

### [API Reference](api.md)
Standard request/response format, error codes, authentication flow, and conventions.

**Topics**: JSON format, Error handling, Auth endpoints, Pagination, Extractors

### [Models & Database](models-database.md)
Complete guide to working with databases, defining models, writing queries, and migrations.

**Topics**: SeaORM entities, Migrations, CRUD operations, Relationships, Transactions

### [Controllers & Routing](controllers-routing.md)
Create API endpoints, handle requests, build responses, and document APIs.

**Topics**: Handlers, Routing, Request handling, Responses, OpenAPI docs

### [Testing](testing.md)
Write unit tests, integration tests, and API tests using Chopin's test utilities.

**Topics**: TestApp, Unit tests, Integration tests, Authentication tests

### [Deployment](deployment.md)
Deploy to production on Docker, AWS, GCP, DigitalOcean, Fly.io, and more.

**Topics**: Docker, Cloud platforms, Database setup, Monitoring, CI/CD

### [Security](security.md)
Security best practices, authentication, authorization, input validation, and hardening.

**Topics**: Auth/authz, Password security, JWT, Input validation, HTTPS, CORS

### [Performance](performance.md)
Optimize for speed with compilation flags, database tuning, caching, and profiling.

**Topics**: Compilation opts, Database indexes, Caching, Profiling, Apple Silicon

### [Caching](caching.md)
Add caching with Redis or in-memory cache. Speed up your API with cache-aside patterns.

**Topics**: CacheService, Redis backend, In-memory cache, TTL, Cache strategies, Best practices

### [File Uploads](file-uploads.md)
Handle multipart file uploads with validation, storage backends, and serving uploaded files.

**Topics**: Multipart forms, UploadedFile, LocalStorage, Custom storage, Image processing, Security

### [Roles & Permissions](roles-permissions.md)
Implement role-based access control (RBAC) with User, Admin, and Superuser roles.

**Topics**: Role hierarchy, AuthUserWithRole, Route protection, Middleware, RBAC testing

### [GraphQL](graphql.md)
Build GraphQL APIs alongside REST endpoints with async-graphql integration.

**Topics**: Schema definition, Authentication, DataLoader, Subscriptions, Testing, Best practices

### [LLM Learning Guide](llm-learning-guide.md)
Complete framework documentation formatted for LLMs. Copy/paste to ChatGPT, Claude, or other AIs to help you build Chopin apps.

**Topics**: Architecture, Models, Controllers, Extractors, Patterns, CLI, Database, Routing, Testing, Deployment, API endpoints, Error handling

---

## 🎯 Common Tasks

### Create a Project

```bash
chopin new my-api
cd my-api
cargo run
```

📖 [Getting Started](getting-started.md#create-your-first-project)

### Generate a Model

```bash
chopin generate model Post title:string body:text published:bool
```

📖 [CLI Reference](cli.md#chopin-generate-model) | [Models Guide](models-database.md#generating-models)

### Create an Endpoint

```rust
async fn handler(
    State(app): State<AppState>,
) -> Result<ApiResponse<Data>, ChopinError> {
    // Your logic
}
```

📖 [Controllers Guide](controllers-routing.md#handler-functions)

### Add Authentication

```rust
async fn protected(
    AuthUser(user_id): AuthUser,
) -> Result<ApiResponse<Data>, ChopinError> {
    // user_id is validated
}
```

📖 [API Reference](api.md#authentication) | [Security Guide](security.md#authentication--authorization)

### Write a Test

```rust
#[tokio::test]
async fn test_feature() {
    let app = TestApp::new().await;
    let response = app.client.get(&app.url("/api/posts")).send().await;
    assert_eq!(response.status, 200);
}
```

📖 [Testing Guide](testing.md#quick-start)

### Deploy with Docker

```bash
docker build -t my-api .
docker run -p 8080:8080 my-api
```

📖 [Deployment Guide](deployment.md#docker-deployment)

---

## 💡 Best Practices

### Development

✅ Use the CLI to generate models consistently  
✅ Write tests for all endpoints  
✅ Use `.env` for local configuration  
✅ Enable `RUST_LOG=debug` during development  
✅ Use `cargo watch` for auto-reload  

### Production

✅ Use strong, random JWT secrets  
✅ Enable HTTPS (reverse proxy)  
✅ Configure CORS properly  
✅ Add database indexes  
✅ Enable monitoring and logging  
✅ Use release builds (`--release`)  

---

## 🔗 External Resources

### Rust Ecosystem

- [Rust Book](https://doc.rust-lang.org/book/) - Learn Rust
- [Async Book](https://rust-lang.github.io/async-book/) - Async programming
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial) - Async runtime

### Framework Dependencies

- [Axum Documentation](https://docs.rs/axum/) - Web framework
- [SeaORM Documentation](https://www.sea-ql.org/SeaORM/) - ORM
- [Tower Documentation](https://docs.rs/tower/) - Middleware

### Tools

- [Cargo Book](https://doc.rust-lang.org/cargo/) - Package manager
- [Clippy](https://github.com/rust-lang/rust-clippy) - Linter
- [Rustfmt](https://github.com/rust-lang/rustfmt) - Formatter

---

## 🆘 Getting Help

### Documentation Issues

- Found a typo or error? [Open an issue](https://github.com/yourusername/chopin/issues)
- Want to improve docs? [Submit a PR](../CONTRIBUTING.md)

### Questions

- Check [Troubleshooting](getting-started.md#troubleshooting) sections
- Search [existing issues](https://github.com/yourusername/chopin/issues)
- Ask in [Discussions](https://github.com/yourusername/chopin/discussions)

### Support

- 📧 Email: support@chopin-framework.dev
- 💬 Discord: [Join our community](https://discord.gg/chopin)
- 🐦 Twitter: [@chopinframework](https://twitter.com/chopinframework)

---

## 📄 License

Chopin is released under the MIT License. See [LICENSE](../LICENSE) for details.

---

## 🙏 Acknowledgments

Chopin is built on the shoulders of giants:

- [Axum](https://github.com/tokio-rs/axum) - Web framework
- [SeaORM](https://github.com/SeaQL/sea-orm) - ORM
- [Tokio](https://github.com/tokio-rs/tokio) - Async runtime
- [serde](https://github.com/serde-rs/serde) - Serialization
- [sonic-rs](https://github.com/cloudwego/sonic-rs) - Fast JSON

Thank you to all contributors and the Rust community!

---

<div align="center">

**[⬆ Back to Top](#chopin-documentation)**

Made with ❤️ by the Chopin team

</div>
