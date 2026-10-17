---
status: akzeptiert
datum: 2026-10-17
---

# Geldbeträge als ganzzahlige Mikro-Dollar

Kostenschätzungen werden als `u64` in Mikro-Dollar gerechnet und gespeichert, nicht als `f64`. Beim Summieren tausender Einzelbeträge entstehen so keine Rundungsfehler; formatiert wird erst in der Anzeige.
