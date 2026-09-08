# CoreLink `image-size` substitute

The documentation build only carries PNG and SVG assets. This bounded
substitute preserves dimension extraction for those formats, rejects all other
formats (including ICNS, JXL, and HEIF), and refuses inputs larger than 10 MiB.
