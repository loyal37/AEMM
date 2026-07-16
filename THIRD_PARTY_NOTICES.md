# Third-party notices

AEMM depends on third-party open-source software. Cargo's resolved versions are recorded in
`src-tauri/Cargo.lock`; the following installation-related dependencies require explicit notice.

## zip

`zip` is copyright its contributors and licensed under the MIT License.

- Source: <https://github.com/zip-rs/zip2>
- License: <https://github.com/zip-rs/zip2/blob/master/LICENSE>

## sevenz-rust2

`sevenz-rust2` is copyright its contributors and licensed under the Apache License 2.0.

- Source: <https://github.com/hasenbanck/sevenz-rust2>
- License: <https://github.com/hasenbanck/sevenz-rust2/blob/main/LICENSE>

## unrar Rust wrapper

The Rust wrapper portions of `unrar` are licensed, at the recipient's option, under the MIT
License or Apache License 2.0.

The RAR adapter unit test includes a Base64 representation derived from the wrapper's
`data/version.rar` test vector under those same terms.

- Source: <https://github.com/muja/unrar.rs>
- License: <https://github.com/muja/unrar.rs#license>

## RARLab UnRAR source

The `unrar` Rust crate statically builds and links the RARLab UnRAR source. That embedded source
is freeware under the following special license. AEMM uses it only to list and extract RAR
archives; it is not used to create a RAR-compatible archiver or reproduce the proprietary RAR
compression algorithm.

> The source code of UnRAR utility is freeware. This means:
>
> 1. All copyrights to RAR and the utility UnRAR are exclusively owned by the author — Alexander
>    Roshal.
>
> 2. UnRAR source code may be used in any software to handle RAR archives without limitations
>    free of charge, but cannot be used to develop RAR (WinRAR) compatible archiver and to
>    re-create RAR compression algorithm, which is proprietary. Distribution of modified UnRAR
>    source code in separate form or as a part of other software is permitted, provided that full
>    text of this paragraph, starting from “UnRAR source code” words, is included in license, or
>    in documentation if license is not available, and in source code comments of resulting
>    package.
>
> 3. The UnRAR utility may be freely distributed. It is allowed to distribute UnRAR inside of
>    other software packages.
>
> 4. THE RAR ARCHIVER AND THE UnRAR UTILITY ARE DISTRIBUTED “AS IS”. NO WARRANTY OF ANY KIND IS
>    EXPRESSED OR IMPLIED. YOU USE AT YOUR OWN RISK. THE AUTHOR WILL NOT BE LIABLE FOR DATA LOSS,
>    DAMAGES, LOSS OF PROFITS OR ANY OTHER KIND OF LOSS WHILE USING OR MISUSING THIS SOFTWARE.
>
> 5. Installing and using the UnRAR utility signifies acceptance of these terms and conditions of
>    the license.
>
> 6. If you don't agree with terms of the license you must remove UnRAR files from your storage
>    devices and cease to use the utility.

The authoritative license text is distributed by RARLab with the embedded source and is also
available at <https://github.com/pmachapman/unrar/blob/master/license.txt>.
