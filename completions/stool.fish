# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_stool_global_optspecs
	string join \n v/verbose h/help V/version
end

function __fish_stool_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_stool_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_stool_using_subcommand
	set -l cmd (__fish_stool_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c stool -n "__fish_stool_needs_command" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_needs_command" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_needs_command" -s V -l version -d 'Print version'
complete -c stool -n "__fish_stool_needs_command" -f -a "init" -d 'Save dotfiles defaults, initialize Apple settings and optionally restore offline keys'
complete -c stool -n "__fish_stool_needs_command" -f -a "apple" -d 'Manage macOS preferences and Homebrew independently of dotfiles'
complete -c stool -n "__fish_stool_needs_command" -f -a "dotfile" -d 'Manage dotfiles from a Git repository'
complete -c stool -n "__fish_stool_needs_command" -f -a "secrets" -d 'Back up and restore GPG and SSH keys independently of dotfiles'
complete -c stool -n "__fish_stool_needs_command" -f -a "completions" -d 'Generate shell completions from the current command definitions'
complete -c stool -n "__fish_stool_needs_command" -f -a "man" -d 'Render the manual, or write all command manuals to a directory'
complete -c stool -n "__fish_stool_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c stool -n "__fish_stool_using_subcommand init" -l dotfile-url -d 'Default Git URL; saved without cloning or contacting the repository' -r
complete -c stool -n "__fish_stool_using_subcommand init" -l key-file -d 'Password-encrypted offline archive produced by stool secrets backup' -r -F
complete -c stool -n "__fish_stool_using_subcommand init" -l brewfile -d 'Trusted local Brewfile (defaults to ~/.Brewfile; missing default is skipped)' -r -F
complete -c stool -n "__fish_stool_using_subcommand init" -l passphrase-file -d 'Private external backup password file (requires --key-file)' -r -F
complete -c stool -n "__fish_stool_using_subcommand init" -l dry-run -d 'Preview without saving configuration, decrypting keys or changing the system'
complete -c stool -n "__fish_stool_using_subcommand init" -l non-interactive -d 'Allow package installation without terminal input; prepare privileges separately'
complete -c stool -n "__fish_stool_using_subcommand init" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand init" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand apple; and not __fish_seen_subcommand_from init brew help" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand apple; and not __fish_seen_subcommand_from init brew help" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand apple; and not __fish_seen_subcommand_from init brew help" -f -a "init" -d 'Set the current user\'s Dock minimization animation to suck'
complete -c stool -n "__fish_stool_using_subcommand apple; and not __fish_seen_subcommand_from init brew help" -f -a "brew" -d 'Initialize Homebrew and install software from a Brewfile'
complete -c stool -n "__fish_stool_using_subcommand apple; and not __fish_seen_subcommand_from init brew help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from init" -l dry-run -d 'Show current preferences and planned changes without writing'
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from init" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from brew" -l prefix -d 'Select a Homebrew prefix instead of HOMEBREW_PREFIX, PATH or the native default' -r -F
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from brew" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from brew" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from brew" -f -a "init" -d 'Reuse Homebrew or install it with the official installer'
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from brew" -f -a "install" -d 'Ensure Homebrew is available, then install a trusted Brewfile without routine upgrades'
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from brew" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from help" -f -a "init" -d 'Set the current user\'s Dock minimization animation to suck'
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from help" -f -a "brew" -d 'Initialize Homebrew and install software from a Brewfile'
complete -c stool -n "__fish_stool_using_subcommand apple; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "init" -d 'Clone a Git URL and save configuration'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "diff" -d 'Preview content and permission changes without writing'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "status" -d 'List pending changes and conflicts'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "apply" -d 'Deploy the local source repository'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "update" -d 'Fast-forward the source repository and deploy changes'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "managed" -d 'List managed destination paths'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "add" -d 'Import existing destination files into source state'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "edit" -d 'Edit source files (encrypted files are decrypted only in a private temporary directory)'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "unmanaged" -d 'List destination files not managed by source'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "verify" -d 'Return nonzero when the destination differs from source'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "doctor" -d 'Diagnose repository, configuration, state and optional executables'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "archive" -d 'Export rendered target files as tar, tar.gz, or zip'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "execute-template" -d 'Evaluate a Jinja template using repository and local data'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "data" -d 'Print merged template data as JSON'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "keygen" -d 'Generate an age identity and print its public recipient'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "cd" -d 'Start a shell in the source repository, or print its path'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and not __fish_seen_subcommand_from init diff status apply update managed add edit unmanaged verify doctor archive execute-template data keygen cd help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from init" -l branch -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from init" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from init" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from init" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from init" -l apply -d 'Deploy after cloning'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from init" -l recurse-submodules -d 'Explicitly enable recursive submodules (the default for new repositories)'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from init" -l no-recurse-submodules -d 'Disable recursive submodule initialization and subsequent updates'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from init" -l builtin -d 'Clone using the built-in Git backend'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from init" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from diff" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from diff" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from diff" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from diff" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from diff" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from status" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from status" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from status" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from status" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from apply" -l prompt -d 'Interactive conflict handling (auto only prompts on a terminal)' -r -f -a "auto\t''
always\t''
never\t''"
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from apply" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from apply" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from apply" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from apply" -l dry-run
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from apply" -l force -d 'Back up and overwrite conflicting destination entries'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from apply" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from apply" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from update" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from update" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from update" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from update" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from update" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from managed" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from managed" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from managed" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from managed" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from managed" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from add" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from add" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from add" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from add" -l template
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from add" -l encrypt
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from add" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from edit" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from edit" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from edit" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from edit" -l apply
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from edit" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from edit" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from unmanaged" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from unmanaged" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from unmanaged" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from unmanaged" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from unmanaged" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from verify" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from verify" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from verify" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from verify" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from verify" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from doctor" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from doctor" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from doctor" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from doctor" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from doctor" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from archive" -l format -r -f -a "tar\t''
tgz\t''
zip\t''"
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from archive" -s o -l output -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from archive" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from archive" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from archive" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from archive" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from archive" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from execute-template" -l file -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from execute-template" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from execute-template" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from execute-template" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from execute-template" -l init
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from execute-template" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from execute-template" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from data" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from data" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from data" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from data" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from data" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from keygen" -l output -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from keygen" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from keygen" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from keygen" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from keygen" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from keygen" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from cd" -l source -d 'Local source repository (saved by init)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from cd" -l destination -d 'Destination directory (defaults to the user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from cd" -l data -d 'Override template data (key=value; JSON values are accepted)' -r
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from cd" -l print
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from cd" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from cd" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "init" -d 'Clone a Git URL and save configuration'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "diff" -d 'Preview content and permission changes without writing'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "status" -d 'List pending changes and conflicts'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "apply" -d 'Deploy the local source repository'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "update" -d 'Fast-forward the source repository and deploy changes'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "managed" -d 'List managed destination paths'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "add" -d 'Import existing destination files into source state'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "edit" -d 'Edit source files (encrypted files are decrypted only in a private temporary directory)'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "unmanaged" -d 'List destination files not managed by source'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "verify" -d 'Return nonzero when the destination differs from source'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "doctor" -d 'Diagnose repository, configuration, state and optional executables'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "archive" -d 'Export rendered target files as tar, tar.gz, or zip'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "execute-template" -d 'Evaluate a Jinja template using repository and local data'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "data" -d 'Print merged template data as JSON'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "keygen" -d 'Generate an age identity and print its public recipient'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "cd" -d 'Start a shell in the source repository, or print its path'
complete -c stool -n "__fish_stool_using_subcommand dotfile; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c stool -n "__fish_stool_using_subcommand secrets; and not __fish_seen_subcommand_from backup restore help" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand secrets; and not __fish_seen_subcommand_from backup restore help" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand secrets; and not __fish_seen_subcommand_from backup restore help" -f -a "backup" -d 'Create one password-encrypted offline key backup outside Git workspaces'
complete -c stool -n "__fish_stool_using_subcommand secrets; and not __fish_seen_subcommand_from backup restore help" -f -a "restore" -d 'Validate and manually restore an offline key backup'
complete -c stool -n "__fish_stool_using_subcommand secrets; and not __fish_seen_subcommand_from backup restore help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from backup" -l input -d 'Source user directory (defaults to the current user environment)' -r -F
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from backup" -l output -d 'New backup file (defaults to stool-keys-<timestamp>.tar.age in the current directory)' -r -F
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from backup" -l passphrase-file -d 'Read the password from a private external file or inherited pipe' -r -F
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from backup" -l gpg -d 'Only GPG keys'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from backup" -l ssh -d 'Only SSH keys'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from backup" -l all -d 'All available key categories (the default)'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from backup" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from backup" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from restore" -l input -d 'Password-encrypted archive created by stool secrets backup' -r -F
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from restore" -l output -d 'Target user directory (defaults to the current user\'s home)' -r -F
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from restore" -l passphrase-file -d 'Read the password from a private external file' -r -F
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from restore" -l gpg -d 'Only GPG keys'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from restore" -l ssh -d 'Only SSH keys'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from restore" -l all -d 'All available key categories (the default)'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from restore" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from restore" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from help" -f -a "backup" -d 'Create one password-encrypted offline key backup outside Git workspaces'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from help" -f -a "restore" -d 'Validate and manually restore an offline key backup'
complete -c stool -n "__fish_stool_using_subcommand secrets; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c stool -n "__fish_stool_using_subcommand completions" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand completions" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand man" -l output -r -F
complete -c stool -n "__fish_stool_using_subcommand man" -s v -l verbose -d 'Show more debugging information'
complete -c stool -n "__fish_stool_using_subcommand man" -s h -l help -d 'Print help'
complete -c stool -n "__fish_stool_using_subcommand help; and not __fish_seen_subcommand_from init apple dotfile secrets completions man help" -f -a "init" -d 'Save dotfiles defaults, initialize Apple settings and optionally restore offline keys'
complete -c stool -n "__fish_stool_using_subcommand help; and not __fish_seen_subcommand_from init apple dotfile secrets completions man help" -f -a "apple" -d 'Manage macOS preferences and Homebrew independently of dotfiles'
complete -c stool -n "__fish_stool_using_subcommand help; and not __fish_seen_subcommand_from init apple dotfile secrets completions man help" -f -a "dotfile" -d 'Manage dotfiles from a Git repository'
complete -c stool -n "__fish_stool_using_subcommand help; and not __fish_seen_subcommand_from init apple dotfile secrets completions man help" -f -a "secrets" -d 'Back up and restore GPG and SSH keys independently of dotfiles'
complete -c stool -n "__fish_stool_using_subcommand help; and not __fish_seen_subcommand_from init apple dotfile secrets completions man help" -f -a "completions" -d 'Generate shell completions from the current command definitions'
complete -c stool -n "__fish_stool_using_subcommand help; and not __fish_seen_subcommand_from init apple dotfile secrets completions man help" -f -a "man" -d 'Render the manual, or write all command manuals to a directory'
complete -c stool -n "__fish_stool_using_subcommand help; and not __fish_seen_subcommand_from init apple dotfile secrets completions man help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from apple" -f -a "init" -d 'Set the current user\'s Dock minimization animation to suck'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from apple" -f -a "brew" -d 'Initialize Homebrew and install software from a Brewfile'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "init" -d 'Clone a Git URL and save configuration'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "diff" -d 'Preview content and permission changes without writing'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "status" -d 'List pending changes and conflicts'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "apply" -d 'Deploy the local source repository'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "update" -d 'Fast-forward the source repository and deploy changes'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "managed" -d 'List managed destination paths'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "add" -d 'Import existing destination files into source state'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "edit" -d 'Edit source files (encrypted files are decrypted only in a private temporary directory)'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "unmanaged" -d 'List destination files not managed by source'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "verify" -d 'Return nonzero when the destination differs from source'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "doctor" -d 'Diagnose repository, configuration, state and optional executables'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "archive" -d 'Export rendered target files as tar, tar.gz, or zip'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "execute-template" -d 'Evaluate a Jinja template using repository and local data'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "data" -d 'Print merged template data as JSON'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "keygen" -d 'Generate an age identity and print its public recipient'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from dotfile" -f -a "cd" -d 'Start a shell in the source repository, or print its path'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from secrets" -f -a "backup" -d 'Create one password-encrypted offline key backup outside Git workspaces'
complete -c stool -n "__fish_stool_using_subcommand help; and __fish_seen_subcommand_from secrets" -f -a "restore" -d 'Validate and manually restore an offline key backup'
