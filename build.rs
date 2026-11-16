// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

fn main() {
    println!("cargo:rerun-if-changed=assets/addresses-icon.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_AUTHORS");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winres::WindowsResource::new();
        resource
            .set_icon("assets/addresses-icon.ico")
            .set("ProductName", "Addresses")
            .set(
                "FileDescription",
                "Local contact management and printable address-label PDFs.",
            )
            .set("InternalName", "addresses_rs")
            .set("OriginalFilename", "addresses_rs.exe")
            .set(
                "Comments",
                "AI-assisted development; models may vary by task.",
            );
        let authors =
            std::env::var("CARGO_PKG_AUTHORS").expect("Cargo did not provide package authors");
        if !authors.trim().is_empty() {
            resource.set("Author", &authors);
        }
        resource
            .compile()
            .expect("failed to embed Windows application resources");
    }
}
