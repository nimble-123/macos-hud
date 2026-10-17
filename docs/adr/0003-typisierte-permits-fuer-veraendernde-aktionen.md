---
status: akzeptiert
datum: 2026-10-17
---

# Typisierte Permits für Fast-Forward und Installation

`GitPort::fast_forward` und die Installation von Updates verlangen einen `FastForwardPermit` bzw. `InstallPermit`, den nur die Domain-Regel erzeugen kann (private Felder, Konstruktor `pub(crate)`, nicht `Clone`, wird verbraucht). So ist eine verändernde Aktion ohne bestandene Regelprüfung nicht kompilierbar; das ist der Kern des wichtigsten Qualitätsziels Safety.

## Konsequenzen

- Der Fast-Forward-Permit pinnt `head_oid` und `target_oid`; der Adapter führt `git merge --ff-only <target_oid>` aus und bricht ab, wenn `HEAD` sich seit der Prüfung bewegt hat.
