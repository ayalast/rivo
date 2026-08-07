# TUI Connectivity Diagnosis — rivo -p works, rivo TUI fails

Ver reporte completo del subagente explore (40 archivos, leader vs headless paths) — resumen ejecutivo:

## Hallazgo clave

- `rivo -p` = embedded in-process (fresh reqwest OnceLock, fresh env)
- `rivo` TUI = leader-proxied (separate OS process, long-lived OnceLock clients in xai-grok-sampler/shared_http.rs)

Hipótesis #1 más probable: leader hereda env incompleto (BYOK env_key / proxy / GROK_EXTRA_CA_BUNDLE) o OnceLock stale.

## Cómo reproducir y confirmar

```powershell
# 1. Probar sin leader (fuerza embedded = mismo que -p):
rivo --no-leader
# Si funciona -> leader env/OnceLock es la causa

# 2. Capturar logs:
$env:RUST_LOG="xai_grok_sampler=debug,xai_grok_http=debug"
rivo --debug-file C:\tmp\rivo-tui.log
# reproducir fallo, luego:
Select-String -Path C:\tmp\rivo-tui.log -Pattern "error sending request|os error|BEARER"

# 3. Verificar env parity:
Get-ChildItem Env: | Where-Object Name -match "META|API_KEY|PROXY|GROK_EXTRA"

# 4. Kill stale leader:
rivo leader kill   # o Task Manager: rivo.exe leader
rivo             # fresh leader
```

## Archivos clave

- pager-bin/src/main.rs:2167 headless vs app/mod.rs:650 leader mode, :885 bounded_connect
- xai-grok-sampler/shared_http.rs:72 OnceLock clients
- xai-grok-shell/agent/model_providers.rs: BYOK fail-closed
- xai-grok-extra-ca/src/lib.rs: OnceLock extra roots

## Status actual (verificado)

- `rivo --version` = `1.0.0 (339a157)` ✅
- `rivo -p "hola"` = works with grok-4.5 and muse-spark headless (transient api.meta.ai rate limit recovered) ✅
- `rivo` TUI con api.meta.ai: reportado como fallo por usuario, requiere test TUI con --debug-file para confirmar si es H1 (env) o H2 (stale leader)
