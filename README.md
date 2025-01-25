# HYV Audio Exporter

---

This is a simple utility inspired by [Genshin Audio Extractor](https://github.com/MeguminSama/genshin-audio-extractor) ported to Rust as a simple educational challenge.

Currently, this program doesn't have many features and doesn't convert the extracted `.wem` files into an usable format yet, but this project comes with vgmstream temporarily bundled.

## Usage
`cargo run --release -- <input_pck_file.pck> <output_dir>`