# Repository bootstrap record

- Upstream: <https://github.com/SanderVocke/perfetto-everywhere>
- SSH origin: `git@github.com:SanderVocke/perfetto-everywhere.git`
- Owner/name: `SanderVocke/perfetto-everywhere`
- Visibility: public
- Default branch: `main`
- Bootstrap workspace commit: `5d6e9325d59bde3214f4d9df816aa442e83fbc66`
- First green CI commit: `a6641c48a8db6bbd5f1158ea0ab047a35c0c78de`
- First green CI run: <https://github.com/SanderVocke/perfetto-everywhere/actions/runs/33281266805>

Creation command:

```bash
gh repo create SanderVocke/perfetto-everywhere \
  --public --source ./impl --remote origin --push
```

The prototype parent registers this repository at `impl/` as a Git submodule.
