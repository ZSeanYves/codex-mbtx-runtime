# Files: async 0.21.3

Import `moonbitlang/async@0.21.3/fs` and call from an async function. All paths
are relative to the actual working directory unless absolute. Preserve input
files unless the task explicitly asks to modify them.

| Operation | Exact call |
|---|---|
| Read text | `@fs.read_file(path).text()` |
| Write text or bytes | `@fs.write_file(path, data)` |
| Existence | `@fs.exists(path)` |
| Create directory | `@fs.mkdir(path, recursive=true)` |
| List child names | `@fs.readdir(path, include_hidden=true, sort=true)` |
| Remove file or symlink | `@fs.remove(path)` |
| Remove directory tree | `@fs.rmdir(path, recursive=true)` |
| Rename | `@fs.rename(from, to, replace=true)` |
| Resolve path | `@fs.realpath(path)` |
| Inspect type | `@fs.kind(path, follow_symlink=false)` |

`readdir` returns names, not full paths. Join the parent explicitly. Its sorting
is not a substitute for a task's specified ordering. `read_dir` and
`remove_file` are not these APIs. Check existence before deleting optional files.
The file APIs raise on failure; handle expected missing/invalid input explicitly.
Use `read_file` data directly when copying binary content (or `.binary()` for Bytes): converting it to text
can lose invalid UTF-8. The verified example copies binary bytes unchanged.

For recursive text search/file enumeration, use the task's safe rg forms where
allowed; see `reference:processes`. Read only the task workspace and registered
resources. Source files and caches are not a way to bypass process policy.
