# Omni-Glass: The Visual Action Engine (VAE)

Omni-Glass is an open-source Visual Action Engine. It turns screen content directly into executable actions. Snip any region of your screen — an error message, a data table, foreign text — and get contextual actions powered by your choice of LLM. Extend it with local MCP plugins.

*(Note: 5-second GIF showing a snip of a terminal error turning into a GitHub issue)*

## Architecture & Features

**Local OCR:** Apple Vision / Windows OCR runs on-device. No screenshots leave your machine.

**Dual input:** Snip a screen region or type a command. Both feed the same pipeline.

**Streaming actions:** First action available in under 1 second. The LLM automatically generates structured arguments for your tools from raw OCR text.

**MCP plugins:** Any MCP server over stdio can add actions. Plugins run inside kernel-level macOS `sandbox-exec` isolation and cannot read your home directory unless explicitly approved.

**BYOK:** Bring your own API key. No proxy, no Omni-Glass servers.

## Included Actions

Omni-Glass ships with several built-in actions and one reference MCP plugin:

**Built-ins:** Explain Error, Fix Error, Export CSV, Explain This, Copy Text, Search Web, and Quick Translate.

**MCP Plugin (GitHub Issues):** Snip a terminal error and Omni-Glass will automatically extract the context and draft a GitHub Issue in your repository.

## Quick Start

Omni-Glass is built with Rust and Tauri.

```bash
# Clone the repository
git clone https://github.com/goshtasb/omni-glass.git
cd omni-glass

# Install dependencies
npm install

# Run the dev build
npm run tauri dev
```

## Building a Plugin

Omni-Glass is extensible via the Model Context Protocol. You don't need to learn a proprietary API—if you can write a standard MCP server in Node or Python, it will work here.

To get started, clone our minimalist template repository:

```bash
git clone https://github.com/goshtasb/omni-glass-plugin-template.git
```

Read the full [Developer Guide](docs/plugin-guide.md) to learn how to declare permissions, handle tool calls, and install your plugin locally.

## Contributing

We are actively looking for contributors to help harden the execution environment:

- Help us break and audit the macOS `sandbox-exec` profile.
- Implement the Windows (AppContainer) and Linux (Bubblewrap) security sandboxes.

## License

MIT
