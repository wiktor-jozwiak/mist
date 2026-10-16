## What

## Cost

RSS / CPU / redraws before and after (`bench/measure.sh`, `mist-ui-bin msg ui stats`), or "no running surface touched".

## Evidence

- [ ] `cargo test --release` in `native/` and `scripts/mistd/`
- [ ] Render check: `mist-ui --png` / `--desk-png` / `--dock-png` + `magick compare -metric AE` against the previous binary (0 px for refactors)
