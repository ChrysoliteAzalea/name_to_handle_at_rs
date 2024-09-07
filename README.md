# ```name_to_handle_at_rs``` -- Rust bindings for ```name_to_handle_at()``` and ```open_by_handle_at()``` system calls in Linux

This library crate provides Rust bindings for Linux system calls that allow the caller to refer to the i-node using a byte array known as file handle. The file handle remains valid for the entire life-time of the i-node.

Some uses of this crate:

* User-space NFS servers can use it to maintain NFS file handles
* Fanotify users can use it if they wish to identify watched files using file handles

To read the documentation, use ```cargo doc```. This is a Linux-only project (```name_to_handle_at()``` and ```open_by_handle_at()``` system calls are Linux-specific).
