# findme
A small command-line utility for finding files and folders by date or size

### Building

Build on any platform with `cargo build --release`

On Windows platforms using the MSVC toolchain (default), rustc will statically link the VC++ runtime, making the executable completely portable.

### Examples

Show help for more details

`findme --help`

Find 10 newest modified files in the current directory

`findme -f -r -n 10 newest-modified .`

Find 300 oldest accessed files or directories on the C: Drive

`findme -f -d -r -n 300 oldest-accessed C:\`

Find 42 largest files in either dir1 or dir2, not recursively

`findme -f -n 42 largest dir1 dir2`

### Contributions

Please feel free to contribute code, report bugs, or provide any other feedback.

If contributing code, the only constraint I have is the project may not use unsafe code for any reason. Unsafe code may still exist in dependencies.
