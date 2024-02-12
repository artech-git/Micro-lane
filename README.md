# Micro-lane 🚓
**A blazing-fast & secure Rust based DNS server ⚡️**

**Overview:**

This repository houses a pure-Rust implementation of a high-performance, flexible DNS server, built with efficiency and maintainability in mind.  Leveraging Rust's powerful features, `dns-server` offers:

- **Native performance:** Rust's compiled nature ensures lightning-fast execution, making it ideal for demanding DNS workloads. ️
- **Security:** Rust's memory safety and built-in security features minimize vulnerabilities and enhance trust. ️
- **Flexibility:** The codebase is designed for customization and extension, allowing you to tailor it to your specific needs. 
- **Cross-platform:** Runs seamlessly on various operating systems, providing wide deployment options. 

**Getting Started:**

1. **Requirements:**
   - Rust toolchain ([https://rustup.rs/](https://rustup.rs/))
   - Cargo package manager (comes with Rust)

2. **Clone the repository:**

3. **Build and run:**

   ```bash
   cargo run --release
   ```

   This will start the server on the default port (53).

## **Configuration:**

The server supports configuration via a YAML file named `config.yml` in the project's root directory. Options include:

- **Port:** Specify the DNS listening port (default: 53)
- **Logging:** Configure logging level and output
- **Zone files:** Define DNS zones and records
- **Forwarders:** Set up DNS forwarding for zones not served locally


**Features:**

- Efficient DNS resolution leveraging Rust's concurrency and low-level control. 
- Customizable DNS zone management for flexible configuration. ️
- Support for various record types (A, AAAA, CNAME, etc.) to meet diverse DNS needs. 
- Clean and well-documented codebase for easy understanding and contribution. 

**Contributing:**

We welcome contributions to make `dns-server` even better. 

**License:**

This project is licensed under the MIT License (see `LICENSE`).
