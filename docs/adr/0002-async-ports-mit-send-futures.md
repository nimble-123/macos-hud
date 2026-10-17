---
status: akzeptiert
datum: 2026-10-17
---

# Async-Ports mit expliziten Send-Futures statt nativem async fn

Statische Ports deklarieren ihre Methoden als `fn … -> impl Future<Output = …> + Send`, dynamisch genutzte Ports (`UpdateSource`, `UsageSource`) liefern `BoxFuture`. Natives `async fn` in Traits garantiert kein `Send`, die Multi-Thread-Runtime von Tokio braucht es aber für `tokio::spawn`; die Box-Allokation ist gegenüber einem Prozessaufruf vernachlässigbar.

## Betrachtete Optionen

- `async-trait`: boxt jeden Aufruf und fügt ein Makro hinzu.
- `trait-variant`: liefert eine Send-Variante, macht den Trait aber nicht dyn-kompatibel.
