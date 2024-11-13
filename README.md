# usaco-cli

![GitHub Release](https://img.shields.io/github/v/release/grimsteel/usaco-cli?logo=github) ![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/grimsteel/usaco-cli/release.yml?logo=githubactions&logoColor=white) ![Crates.io Version](https://img.shields.io/crates/v/usaco-cli?logo=rust) ![GitHub License](https://img.shields.io/github/license/grimsteel/usaco-cli) ![GitHub commit activity](https://img.shields.io/github/commit-activity/m/grimsteel/usaco-cli?logo=git&logoColor=white)



A command line tool for USACO

**Features**:
- Account information
- View problem info from command line
- Scaffold solution code
- Automatically test solutions with sample input cases
- Upload solution code to USACO and view results (coming soon)
- View solution stats and find [new problems to solve](https://github.com/imgroot2/algo) (coming soon)

**Supported languages**:
- C++ 17
- Python 3

## Installation

### Source

1. Clone this repo
2. `cargo build --release`

### crates.io

1. `cargo install usaco-cli`

### Binaries

Prebuilt binaries for `x86_64-unknown-linux-gnu` are provided on the Releases page

Make a GH issue if you want more targets

Note that currently, the code only supports UNIX targets, but in the future, I may add Windows support.

## Usage

```sh
$ usaco --help
USACO command-line interface: supports viewing problem info, automatically testing solutions, and uploading solutions to USACO grading servers.

Usage: usaco [OPTIONS] <COMMAND>

Commands:
  auth         Manage USACO account authentication
  problem      View problem info
  solution     Manage, test, and submit solutions
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
