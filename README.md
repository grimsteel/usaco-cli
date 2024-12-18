# usaco-cli

![GitHub Release](https://img.shields.io/github/v/release/grimsteel/usaco-cli?logo=github) ![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/grimsteel/usaco-cli/release.yml?logo=githubactions&logoColor=white) ![Crates.io Version](https://img.shields.io/crates/v/usaco-cli?logo=rust) ![GitHub License](https://img.shields.io/github/license/grimsteel/usaco-cli) ![GitHub commit activity](https://img.shields.io/github/commit-activity/m/grimsteel/usaco-cli?logo=git&logoColor=white)

![demo gif](demo/demo.gif)

A command line tool for USACO

**Features**:
- Account information
- View problem info from command line
- Scaffold solution code
- Automatically test solutions with sample input cases
- View solution stats and find [new problems to solve](https://github.com/imgroot2/algo) (coming soon)

**Supported languages**:
- C++ 17
- Python 3

> [!WARNING]
> This is an unofficial tool, and is neither endorsed nor supported by USACO.
> 
> I would not recommend using this during official competitions.
> 
> I am not responsible for any consequences from using this tool.

## Installation

### Source

1. Clone this repo
2. `cargo build --release`

### crates.io

1. `cargo install usaco-cli`

### Binaries

Prebuilt binaries for `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, and `x86_64-pc-windows-msvc` are provided on the Releases page

Make a GH issue if you want more targets

## Usage

```sh
$ usaco --help
USACO command-line interface: supports viewing problem info, automatically testing solutions, and viewing test case diffs.

Usage: usaco [OPTIONS] <COMMAND>

Commands:
  auth         Manage USACO account authentication
  problem      View problem info
  solution     Manage and test solutions
  preferences  Manage CLI preferences
  completion   Generate shell completion files
  ping         Test connection to USACO servers
  help         Print this message or the help of the given subcommand(s)

Options:
  -l, --log-level <LOG_LEVEL>
          Maximum logging level

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
