```
             ._     __,
              |\,../'\
            ,'. .     `.
           .--         '`.
          ( `' ,          ;
          ,`--' _,       ,'\
         ,`.____            `.
        /              `,    |
       '                \,   '
       |                /   /`,
       `,  .           ,` ./  |
       ' `.  ,'        |;,'   ,@
 ______|     |      _________,_____jv______
        `.   `.   ,'
         ,'_,','_,
         '   `'
```

# landing pig CLI

Terminal AI agent for landing page engineering — blue-accent TUI, Anthropic API, workspace import, and model picker.

> [!NOTE]
> This repository is the official distribution channel for **landingpig**. The installer clones this repo, compiles the Rust CLI on your machine, and adds `landingpig` to your PATH.

## Installation

**One command** (Linux, macOS, WSL, or Git Bash on Windows):

```bash
curl -fsSL https://raw.githubusercontent.com/Mike4947/landingpig/main/install.sh | bash
```

The wizard will ask where to install (default: main drive), check disk space, build the binary, and configure your shell PATH. Open a **new terminal** and run:

```bash
landingpig
```

On first launch, paste your Anthropic API key. Config is stored at `~/.config/landingpig/config.json`.

**Requirements:** `git`, `curl`, and the [Rust toolchain](https://rustup.rs) (`cargo`).

**Custom install location** (after cloning the repo):

```bash
./install.sh --prefix /path/to/landingpig
```

## Commands

| Command | Description |
|---------|-------------|
| `/import` | Open workspace picker |
| `/import <path>` | Import path directly |
| `/model` | Open model picker (+ reasoning toggle) |
| `/model <id>` | Switch model directly |
| `/design <brief>` | Design a landing page from a brief |
| `/redesign [prompt]` | AI redesign loop |
| `/read <file>` | Read workspace file |
| `/write <file>` | Write last output |
| `/help` | Show help |
| `Esc` (while generating) | Stop generation |

## Screenshots

<img width="1246" height="667" alt="Main menu" src="https://github.com/user-attachments/assets/7eb3929d-b3cd-4563-92d6-362eaa0cbfe5" />

<img width="1246" height="665" alt="CLI" src="https://github.com/user-attachments/assets/a25300ac-9091-47ad-b629-70e978287fde" />

## License

MIT
