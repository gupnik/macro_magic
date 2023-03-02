# Macro Magic 🪄

This crate provides two powerful proc macros, `#[export_tokens]` and `import_tokens!`. When
used in tandem, these two macros allow you to mark items in other files (and even in other
crates, as long as you can modify the source code) for export. The tokens of these items can
then be imported by the `import_tokens!` macro using the path to an item you have exported.

An advanced macro, `import_tokens_indirect!` is also provided which is capable of going across
crate boundaries without complicating your dependencies.

Among other things, the patterns introduced by Macro Magic can be used to implement safe and
efficient coordination and communication between macro invocations in the same file, and even
across different files and different crates. This crate officially supercedes my previous
effort at achieving this, [macro_state](https://crates.io/crates/macro_state), which was
designed to allow for building up and making use of state information across multiple macro
invocations. All of the things you can do with `macro_state` you can also achieve with this
crate, albeit with slightly different patterns.

Macro Magic is designed to work with stable Rust.

## Example

Let's say you have some module that defines a bunch of type aliases like this:

```rust
// src/bar/baz.rs

pub mod foo {
    type Foo = u32;
    type Bar = usize;
    type Fizz = String;
    type Buzz = bool;
}
```

And let's say you are writing some proc macro somewhere else, and you realize you really need
to know what types have and have not been defined in the `bar::baz::foo` module shown above,
perhaps so you can provide default values for these type aliases if they are not present.

```rust
#[proc_macro]
pub fn my_macro(tokens: TokenStream) -> TokenStream {
    // ...
    let foo_tokens: TokenStream2 = ???
    // ...
}
```

We need the tokens from some item that hasn't been passed to our macro here. How can we get
them?

Well, you can attach the `#[export_tokens]` attribute macro to the `foo` module as follows:

```rust
// src/bar/baz.rs

use macro_magic::export_tokens;

#[export_tokens]
pub mod foo {
    type Foo = u32;
    // ...
}
```

Now you can import the tokens for the entire `foo` module inside of `my_macro` even though
they are in different crates. The only caveat is that you have to import the `foo` module to
the context where you are writing your macro, like so:

```rust
use bar::baz::foo;

use macro_magic::import_tokens;

#[proc_macro]
pub fn my_macro(tokens: TokenStream) -> TokenStream {
    let foo_tokens: TokenStream = import_tokens!(foo).into(); // type is TokenStream2
    let parsed_mod: ItemMod = parse_macro_input!(foo_tokens as ItemMod);
    // ...
}
```

Even this caveat can be removed if you make use of indirect imports (explained below), which
are capable of working without requiring the token source to be a dependency of the target.

## `#[export_tokens]`

You can apply the `#[export_tokens]` macro to any
[Item](https://docs.rs/syn/latest/syn/enum.Item.html), with the exception of foreign modules,
impls, unnamed decl macros, and use declarations.

When you apply `#[export_tokens]` to an item, a `const` variable is generated immediately after
the item and set to a `&'static str` containing the source code of the item. The `const`
variable is hidden from docs and its name consists of the upcased item name (i.e. the ident),
prefixed with `__EXPORT_TOKENS__`, to avoid any collisions with any legitimate constants that
may have been defined.

This allows the tokens for the item to be imported using the `import_tokens!` macro.

Optionally, you may specify a disambiguation path for the item as an argument to the macro,
such as:

```rust
#[export_tokens(my::cool::ItemName)]
fn my_item () {
    // ...
}
```

Any valid `syn::TypePath`-compatible item is acceptable as input for `#[export_tokens]` and
this input is optional. Furthermore the path need not exist -- you just have to use the same
path when you import.

### Expansion

```rust
#[export_tokens]
fn foo_bar(a: u32) -> u32 {
    a * 2
}
```

expands to:

```rust
#[allow(dead_code)]
fn foo_bar(a: u32) -> u32 {
    a * 2
}
#[allow(dead_code)]
#[doc(hidden)]
pub const __EXPORT_TOKENS__FOO_BAR: &'static str = "fn foo_bar(a : u32) -> u32 { a * 2 }";
```

NOTE: items marked with `#[export_tokens]` do not need to be public, however they do need to be
in a module that is accessible from wherever you intend to call `import_tokens!`.

## `import_tokens!`

You can pass the path of any item that has had the `#[export_tokens]` attribute applied to it
directly to the `import_tokens!` macro to get a
[TokenStream2](https://docs.rs/proc-macro2/latest/proc_macro2/struct.TokenStream.html) of the
foreign item.

For example, suppose the `foo_bar` function mentioned above is located in another crate and can
be accessed via `really::cool::path::foo_bar`. As long as that path is accessible from the
current context (i.e. could be loaded via a `use` statement if you wanted to), `import_tokens!`
will expand to a `TokenStream2` of the item, e.g.:

```rust
let tokens = import_tokens!(cool::path::foo_bar);
```

This style of importing is called a direct import because we directly include the code we are
exporting into the context where the tokens are being used (usually a proc macro crate).

### Expansion

The example above would roughly expand to:

```rust
let tokens = cool::path::__EXPORT_TOKENS__FOO_BAR.parse::<TokenStream2>().unwrap();
```

## `import_tokens_indirect!`

While direct imports are useful, there are situations where it would be impractical or
extremely cumbersome to have the crate where your tokens are exported from (i.e. the "source"
crate) be a dependency of your proc macro crate where those tokens are used (i.e. the "target
crate"). This is especially true in scenarios where your proc macro crate is consumed by
arbitrary downstream users who cannot modify your proc macro crate in any way without forking
it. We provide a workaround via what we call "indirect imports". Another use-case for indirect
imports is scenarios where the item in question is hidden behind a private module, as indirect
imports can work around this scenario.

Calling `import_tokens_indirect!` is slightly different from calling `import_tokens!` in that
indirect imports will work even when the item whose tokens you are importing is contained in a
crate that is not a dependency of the current crate, so long as the following requirements are
met:

1. The source crate and the target crate must be in the same
   [cargo workspace](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html). This is a
   non-negotiable hard requirement when using indirect imports, however direct imports will
   work fine across workspace boundaries (they just have other stricter requirements that can
   be cumbersome).
2. The source crate and the target crate must both use the same version of `macro_magic` (this
   is not a hard requirement, but undefined behavior could occur with mixed versions).
3. Both the source crate and target crate must be included in the compilation target of the
   current workspace such that they are both compiled. Unlike with direct imports, where you
   explictily `use` the source crate as a dependency of the target crate, there needs to be
   some reason to compile the source crate, or its exported tokens will be unavailable.
4. The export path declared by the source crate must exactly match the path you try to import
   in the target crate. If you don't manually specify an export path, then your import path
   should be the name of the item that `#[export_tokens]` was attached to (i.e. the `Ident`),
   however this approach is not recommended since you can run into collisions if you are not
   explicit about naming. For highly uniquely named items, however, this is fine.

The vast majority of common use cases for `macro_magic` meet these criteria, but if you run
into any issues where exported tokens can't be found, make sure your source crate is included
as part of the compilation target and that it is in the current workspace.

Keep in mind that you can use the optional attribute, `#[export_tokens(my::path::Here)]` to
specify a disambiguation path for the tokens you are exporting. Otherwise the name of the item
the macro is attached to will be used, potentially causing collisions if you export items by
the same name from different contexts.

This situation will eventually be resolved when the machinery behind
[caller_modpath](https://crates.io/crates/caller_modpath) is stabilized, which will allow
`macro_magic` to automatically detect the path of the `#[export_tokens]` caller.

A peculiar aspect of how `#[export_tokens(some_path)]` works is the path you enter doesn't need
to be a real path. You could do `#[export_tokens(completely::made_up::path::MyItem)]` in one
context and then `import_tokens!(completely::made_up::path::MyItem)` in another context, and it
will still work as long as these two paths are the same. They need not actually exist, they are
just use for disambiguation so we can tell the difference between these tokens and other
potential exports of an item called `MyItem`. The last segment _does_ need to match the name of
the item you are exporting, however.

## Overhead

Because the automatically generated constants created by `#[export_tokens]` are only used in a
proc-macro context, these constants do not add any bloat to the final binary because they will
be optimized out in contexts where they are not used. Thus these constants are a zero-overhead
abstraction once proc-macro expansion completes. The same goes for the temporary files used by
the indirect imports approach. These artifacts only exist at compile time and do not make it
into the final binary.

On a micro-scale, direct imports are slightly more efficient than indirect imports because they
do not involve any extra IO activity, using only a `const` to synchronize information between
source and target.

## Safety

Direct imports via `import_tokens!` are 100% safe and don't rely on anything sketchy about
compile-order or artifacts in the `target` directory.

Indirect imports are also safe because of how the `macro_magic` build script is constructed
(unlike `macro_state`, which may stop working in the future depending on what changes are made
to the Rust language), however, under the hood indirect imports do rely on coordinating based
on files in the `target` directory for the current workspace, so mileage may vary depending on
the context where you try to use this approach.

For this reason you should stick with direct imports via `import_tokens!` unless your use case
requires the extra flexibility provided by `import_tokens_indirect!`.
