
use builtin;
use str;

set edit:completion:arg-completer[stool] = {|@words|
    fn spaces {|n|
        builtin:repeat $n ' ' | str:join ''
    }
    fn cand {|text desc|
        edit:complex-candidate $text &display=$text' '(spaces (- 14 (wcswidth $text)))$desc
    }
    var command = 'stool'
    for word $words[1..-1] {
        if (str:has-prefix $word '-') {
            break
        }
        set command = $command';'$word
    }
    var completions = [
        &'stool'= {
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
            cand init 'Save dotfiles defaults, initialize Apple settings and optionally restore offline keys'
            cand apple 'Manage macOS preferences and Homebrew independently of dotfiles'
            cand dotfile 'Manage dotfiles from a Git repository'
            cand secrets 'Back up and restore GPG and SSH keys independently of dotfiles'
            cand completions 'Generate shell completions from the current command definitions'
            cand man 'Render the manual, or write all command manuals to a directory'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'stool;init'= {
            cand --dotfile-url 'Default Git URL; saved without cloning or contacting the repository'
            cand --key-file 'Password-encrypted offline archive produced by stool secrets backup'
            cand --brewfile 'Trusted local Brewfile (defaults to ~/.Brewfile; missing default is skipped)'
            cand --passphrase-file 'Private external backup password file (requires --key-file)'
            cand --dry-run 'Preview without saving configuration, decrypting keys or changing the system'
            cand --non-interactive 'Allow package installation without terminal input; prepare privileges separately'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;apple'= {
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
            cand init 'Set the current user''s Dock minimization animation to suck'
            cand brew 'Initialize Homebrew and install software from a Brewfile'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'stool;apple;init'= {
            cand --dry-run 'Show current preferences and planned changes without writing'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;apple;brew'= {
            cand --prefix 'Select a Homebrew prefix instead of HOMEBREW_PREFIX, PATH or the native default'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
            cand init 'Reuse Homebrew or install it with the official installer'
            cand install 'Ensure Homebrew is available, then install a trusted Brewfile without routine upgrades'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'stool;apple;brew;init'= {
            cand --prefix 'Select a Homebrew prefix instead of HOMEBREW_PREFIX, PATH or the native default'
            cand --dry-run 'Show the plan without downloading, installing or evaluating the Brewfile'
            cand --non-interactive 'Run without terminal input (requires privileges to be prepared separately)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;apple;brew;install'= {
            cand --file 'Trusted Brewfile to install (defaults to ~/.Brewfile)'
            cand --prefix 'Select a Homebrew prefix instead of HOMEBREW_PREFIX, PATH or the native default'
            cand --dry-run 'Show the plan without downloading, installing or evaluating the Brewfile'
            cand --non-interactive 'Run without terminal input (requires privileges to be prepared separately)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;apple;brew;help'= {
            cand init 'Reuse Homebrew or install it with the official installer'
            cand install 'Ensure Homebrew is available, then install a trusted Brewfile without routine upgrades'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'stool;apple;brew;help;init'= {
        }
        &'stool;apple;brew;help;install'= {
        }
        &'stool;apple;brew;help;help'= {
        }
        &'stool;apple;help'= {
            cand init 'Set the current user''s Dock minimization animation to suck'
            cand brew 'Initialize Homebrew and install software from a Brewfile'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'stool;apple;help;init'= {
        }
        &'stool;apple;help;brew'= {
            cand init 'Reuse Homebrew or install it with the official installer'
            cand install 'Ensure Homebrew is available, then install a trusted Brewfile without routine upgrades'
        }
        &'stool;apple;help;brew;init'= {
        }
        &'stool;apple;help;brew;install'= {
        }
        &'stool;apple;help;help'= {
        }
        &'stool;dotfile'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
            cand init 'Clone a Git URL and save configuration'
            cand diff 'Preview content and permission changes without writing'
            cand status 'List pending changes and conflicts'
            cand apply 'Deploy the local source repository'
            cand update 'Fast-forward the source repository and deploy changes'
            cand managed 'List managed destination paths'
            cand add 'Import existing destination files into source state'
            cand edit 'Edit source files (encrypted files are decrypted only in a private temporary directory)'
            cand unmanaged 'List destination files not managed by source'
            cand verify 'Return nonzero when the destination differs from source'
            cand doctor 'Diagnose repository, configuration, state and optional executables'
            cand archive 'Export rendered target files as tar, tar.gz, or zip'
            cand execute-template 'Evaluate a Jinja template using repository and local data'
            cand data 'Print merged template data as JSON'
            cand keygen 'Generate an age identity and print its public recipient'
            cand cd 'Start a shell in the source repository, or print its path'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'stool;dotfile;init'= {
            cand --branch 'branch'
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand --apply 'Deploy after cloning'
            cand --recurse-submodules 'Explicitly enable recursive submodules (the default for new repositories)'
            cand --no-recurse-submodules 'Disable recursive submodule initialization and subsequent updates'
            cand --builtin 'Clone using the built-in Git backend'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;diff'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;status'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;apply'= {
            cand --prompt 'Interactive conflict handling (auto only prompts on a terminal)'
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand --dry-run 'dry-run'
            cand --force 'Back up and overwrite conflicting destination entries'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;update'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;managed'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;add'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand --template 'template'
            cand --encrypt 'encrypt'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;edit'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand --apply 'apply'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;unmanaged'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;verify'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;doctor'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;archive'= {
            cand --format 'format'
            cand -o 'o'
            cand --output 'output'
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;execute-template'= {
            cand --file 'file'
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand --init 'init'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;data'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;keygen'= {
            cand --output 'output'
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;cd'= {
            cand --source 'Local source repository (saved by init)'
            cand --destination 'Destination directory (defaults to the user''s home)'
            cand --data 'Override template data (key=value; JSON values are accepted)'
            cand --print 'print'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;dotfile;help'= {
            cand init 'Clone a Git URL and save configuration'
            cand diff 'Preview content and permission changes without writing'
            cand status 'List pending changes and conflicts'
            cand apply 'Deploy the local source repository'
            cand update 'Fast-forward the source repository and deploy changes'
            cand managed 'List managed destination paths'
            cand add 'Import existing destination files into source state'
            cand edit 'Edit source files (encrypted files are decrypted only in a private temporary directory)'
            cand unmanaged 'List destination files not managed by source'
            cand verify 'Return nonzero when the destination differs from source'
            cand doctor 'Diagnose repository, configuration, state and optional executables'
            cand archive 'Export rendered target files as tar, tar.gz, or zip'
            cand execute-template 'Evaluate a Jinja template using repository and local data'
            cand data 'Print merged template data as JSON'
            cand keygen 'Generate an age identity and print its public recipient'
            cand cd 'Start a shell in the source repository, or print its path'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'stool;dotfile;help;init'= {
        }
        &'stool;dotfile;help;diff'= {
        }
        &'stool;dotfile;help;status'= {
        }
        &'stool;dotfile;help;apply'= {
        }
        &'stool;dotfile;help;update'= {
        }
        &'stool;dotfile;help;managed'= {
        }
        &'stool;dotfile;help;add'= {
        }
        &'stool;dotfile;help;edit'= {
        }
        &'stool;dotfile;help;unmanaged'= {
        }
        &'stool;dotfile;help;verify'= {
        }
        &'stool;dotfile;help;doctor'= {
        }
        &'stool;dotfile;help;archive'= {
        }
        &'stool;dotfile;help;execute-template'= {
        }
        &'stool;dotfile;help;data'= {
        }
        &'stool;dotfile;help;keygen'= {
        }
        &'stool;dotfile;help;cd'= {
        }
        &'stool;dotfile;help;help'= {
        }
        &'stool;secrets'= {
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
            cand backup 'Create one password-encrypted offline key backup outside Git workspaces'
            cand restore 'Validate and manually restore an offline key backup'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'stool;secrets;backup'= {
            cand --input 'Source user directory (defaults to the current user environment)'
            cand --output 'New backup file (defaults to stool-keys-<timestamp>.tar.age in the current directory)'
            cand --passphrase-file 'Read the password from a private external file or inherited pipe'
            cand --gpg 'Only GPG keys'
            cand --ssh 'Only SSH keys'
            cand --all 'All available key categories (the default)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;secrets;restore'= {
            cand --input 'Password-encrypted archive created by stool secrets backup'
            cand --output 'Target user directory (defaults to the current user''s home)'
            cand --passphrase-file 'Read the password from a private external file'
            cand --gpg 'Only GPG keys'
            cand --ssh 'Only SSH keys'
            cand --all 'All available key categories (the default)'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;secrets;help'= {
            cand backup 'Create one password-encrypted offline key backup outside Git workspaces'
            cand restore 'Validate and manually restore an offline key backup'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'stool;secrets;help;backup'= {
        }
        &'stool;secrets;help;restore'= {
        }
        &'stool;secrets;help;help'= {
        }
        &'stool;completions'= {
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;man'= {
            cand --output 'output'
            cand -v 'Show more debugging information'
            cand --verbose 'Show more debugging information'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'stool;help'= {
            cand init 'Save dotfiles defaults, initialize Apple settings and optionally restore offline keys'
            cand apple 'Manage macOS preferences and Homebrew independently of dotfiles'
            cand dotfile 'Manage dotfiles from a Git repository'
            cand secrets 'Back up and restore GPG and SSH keys independently of dotfiles'
            cand completions 'Generate shell completions from the current command definitions'
            cand man 'Render the manual, or write all command manuals to a directory'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'stool;help;init'= {
        }
        &'stool;help;apple'= {
            cand init 'Set the current user''s Dock minimization animation to suck'
            cand brew 'Initialize Homebrew and install software from a Brewfile'
        }
        &'stool;help;apple;init'= {
        }
        &'stool;help;apple;brew'= {
            cand init 'Reuse Homebrew or install it with the official installer'
            cand install 'Ensure Homebrew is available, then install a trusted Brewfile without routine upgrades'
        }
        &'stool;help;apple;brew;init'= {
        }
        &'stool;help;apple;brew;install'= {
        }
        &'stool;help;dotfile'= {
            cand init 'Clone a Git URL and save configuration'
            cand diff 'Preview content and permission changes without writing'
            cand status 'List pending changes and conflicts'
            cand apply 'Deploy the local source repository'
            cand update 'Fast-forward the source repository and deploy changes'
            cand managed 'List managed destination paths'
            cand add 'Import existing destination files into source state'
            cand edit 'Edit source files (encrypted files are decrypted only in a private temporary directory)'
            cand unmanaged 'List destination files not managed by source'
            cand verify 'Return nonzero when the destination differs from source'
            cand doctor 'Diagnose repository, configuration, state and optional executables'
            cand archive 'Export rendered target files as tar, tar.gz, or zip'
            cand execute-template 'Evaluate a Jinja template using repository and local data'
            cand data 'Print merged template data as JSON'
            cand keygen 'Generate an age identity and print its public recipient'
            cand cd 'Start a shell in the source repository, or print its path'
        }
        &'stool;help;dotfile;init'= {
        }
        &'stool;help;dotfile;diff'= {
        }
        &'stool;help;dotfile;status'= {
        }
        &'stool;help;dotfile;apply'= {
        }
        &'stool;help;dotfile;update'= {
        }
        &'stool;help;dotfile;managed'= {
        }
        &'stool;help;dotfile;add'= {
        }
        &'stool;help;dotfile;edit'= {
        }
        &'stool;help;dotfile;unmanaged'= {
        }
        &'stool;help;dotfile;verify'= {
        }
        &'stool;help;dotfile;doctor'= {
        }
        &'stool;help;dotfile;archive'= {
        }
        &'stool;help;dotfile;execute-template'= {
        }
        &'stool;help;dotfile;data'= {
        }
        &'stool;help;dotfile;keygen'= {
        }
        &'stool;help;dotfile;cd'= {
        }
        &'stool;help;secrets'= {
            cand backup 'Create one password-encrypted offline key backup outside Git workspaces'
            cand restore 'Validate and manually restore an offline key backup'
        }
        &'stool;help;secrets;backup'= {
        }
        &'stool;help;secrets;restore'= {
        }
        &'stool;help;completions'= {
        }
        &'stool;help;man'= {
        }
        &'stool;help;help'= {
        }
    ]
    $completions[$command]
}
