#!/bin/sh
set -eu
# Run after building stool when changing the clap command definitions.
stool_binary=${1:-target/debug/stool}
"$stool_binary" completions bash > completions/stool.bash
"$stool_binary" completions zsh > completions/_stool
"$stool_binary" completions fish > completions/stool.fish
"$stool_binary" completions powershell > completions/_stool.ps1
"$stool_binary" completions elvish > completions/stool.elv
"$stool_binary" man --output man-page
