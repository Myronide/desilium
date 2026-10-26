# WC

Ein **Floating-first Wayland-Compositor in Rust**, mit nativen, grafischen
Einstellungen auf **Dioxus Native / Blitz**. Fenster stehen im Mittelpunkt:
überlappen, verschieben, skalieren, maximieren und einrasten. Kein Browser,
keine WebView, kein JavaScript für die Oberfläche.

**Stand: funktionierender Nested-Prototyp für Linux, kein fertiger KWin-Ersatz.**
„Windows-prioritized“ ist hier als fensterorientierte Bedienung umgesetzt,
nicht als Unterstützung des Microsoft-Windows-Betriebssystems.

Der ursprüngliche WC-Prototyp ist hier als eigenständiger Workspace integriert.
Siehe [Integrationsübersicht](../../docs/wc-integration.md).

## Starten

Benötigt einen laufenden Wayland- oder X11-Desktop, ein gültiges
`XDG_RUNTIME_DIR`, ein aktuelles Rust stable, C/C++-Buildwerkzeuge, pkg-config,
Clang/libclang, Wayland, xkbcommon, EGL/Mesa, OpenSSL und Systemschriften.
Die Blitz-GUI braucht einen funktionierenden Vulkan-Treiber.

Auf Arch Linux beispielsweise:

```sh
sudo pacman -S --needed rust base-devel clang pkgconf wayland libxkbcommon mesa libglvnd openssl fontconfig
# Zusätzlich den passenden Vulkan-Treiber deiner GPU installieren.
```

Auf Debian/Ubuntu sind die Entwicklungsbibliotheken in
[.github/workflows/ci.yml](.github/workflows/ci.yml) aufgeführt.

```sh
./scripts/dev.sh
```

Das baut alle vier Programme und öffnet den Compositor mit Einstellungen und
Demofenster innerhalb eines separaten Desktopfensters. Die laufende Sitzung
wird nicht ersetzt. Der erste Build lädt die Rust-Abhängigkeiten herunter.

Manuell:

```sh
cargo build --workspace --locked
./target/debug/wc-compositor --settings --command "$PWD/target/debug/wc-demo"
```

Der Compositor gibt seinen **eigenen** `WAYLAND_DISPLAY` aus. Weitere Clients
lassen sich damit starten:

```sh
WAYLAND_DISPLAY=wayland-N ./target/debug/wc-demo
# N durch die tatsächlich ausgegebene Socketnummer ersetzen.
```

`--terminal` startet zusätzlich das konfigurierte Terminal. Als Standard wird
das erste vorhandene Programm aus `foot`, `kitty`, `alacritty`,
`weston-terminal` gewählt. Programmargumente und Shell-Befehle werden nicht
interpretiert. Fehlgeschlagene Programmstarts erscheinen im Log.

Die Einstellungen können auch außerhalb des Compositors geöffnet werden:

```sh
./target/debug/wc-settings
```

## Bedienung

Alle folgenden Tastenkürzel sind in **Shortcuts** änderbar:

| Aktion | Standard |
| --- | --- |
| Terminal | Super + Return |
| Einstellungen | Super + Komma |
| Fenster wechseln | Alt + Tab |
| Fenster schließen | Alt + F4 |
| Maximieren | Super + Pfeil hoch |
| Ursprüngliche Größe und Position | Super + Pfeil runter |
| Linke / rechte Bildschirmhälfte | Super + Pfeil links / rechts |
| Compositor beenden | Super + Shift + Escape |

Die Taste Super ist üblicherweise die Windows-Taste. Der äußere Compositor
kann globale Kürzel bereits abfangen; dann andere Kürzel in WC einstellen.
`Alt+Tab` rotiert die Stapelreihenfolge, es gibt noch keinen grafischen Switcher.

- Titelleiste ziehen: verschieben; Rand oder Ecke der Anwendung ziehen: skalieren.
- Super + linke Maustaste ziehen: beliebiges Fenster verschieben.
- Super + rechte Maustaste ziehen: beliebiges Fenster skalieren.
- Am linken/rechten Rand loslassen: Bildschirmhälfte. Oben: maximieren.
- Maximierte/eingerastete Fenster lassen sich wieder herausziehen.
- Demofenster: eigene Titelleiste mit Maximieren/Schließen; unten rechts skalieren.

## Grafische Einstellungen

Vier Seiten mit Seitensuche, Entwurf, Validierung, Verwerfen,
Standardwerten und explizitem Übernehmen:

- **Fenster:** Klick- oder Mausfokus, Anheben bei Mausfokus,
  Einrasten, Randabstand und Fensterabstände.
- **Tastatur:** Wiederholungsrate, Verzögerung und Terminalprogramm.
- **Design:** Hintergrundfarbe, Farbvorlagen und Vorschau.
- **Shortcuts:** neun frei belegbare Aktionen; doppelte und ungültige Belegungen
  werden abgelehnt. Namen wie `Super+Return`, `Alt+F4`, `Ctrl+Alt+t`.

**Übernehmen** validiert und speichert zuerst. Erst nach erfolgreichem Speichern
wird die laufende Instanz geändert. Eingerastete Fenster werden dabei neu
angeordnet; auch beim Vergrößern des äußeren Desktopfensters.

Die Konfiguration liegt in `$XDG_CONFIG_HOME/wc/settings.toml`, sonst
`~/.config/wc/settings.toml`. Schreiben erfolgt über eine temporäre Datei und
atomare Umbenennung. Fehlerhafte bestehende Dateien führen beim Start zu einer
verständlichen Fehlermeldung und werden nicht automatisch überschrieben.
Ohne laufenden Compositor bietet die GUI explizites **Offline speichern** an.
Offline gespeicherte Werte gelten beim nächsten Compositorstart.

## Aufbau

| Crate | Aufgabe |
| --- | --- |
| `wc-compositor` | Smithay-Waylandserver, Winit/EGL-Backend, GLES-Komposition, Fensterpolitik, Input, DMA-BUF und IPC |
| `wc-settings` | Dioxus Native / Blitz; dieselben validierten Einstellungen wie der Server |
| `wc-core` | Konfiguration, Tastenkürzel, Geometrie, IPC-Typen und `wcctl` |
| `wc-demo` | Echter Wayland-Client mit CPU-/SHM-Rendering und eigener Titelleiste |

Die GUI läuft regulär **innerhalb** von WC und rendert GPU-beschleunigt.
Der Compositor prüft DMA-BUF-Importe mit seinem Renderer und stellt nach
Möglichkeit DMA-BUF-v4-Feedback mit Render-Node bereit; andernfalls v3.
Der Democlient funktioniert auch ohne GPU-Pufferimport.

Unterstützt sind XDG-Toplevels, Popups ohne Grab-Management, subsurfaces,
SHM, DMA-BUF, Tastatur, Maus, Scrollen, Clipboard-Basis, Move-/Resize-Grabs,
Clientdekorationen, Cursor-Surfaces und Cursor-Shape. Vollbildanforderungen von
Clients werden berücksichtigt. Der Nested-Output ist derzeit der einzige Monitor.

Smithay ist auf einen konkreten Git-Commit festgelegt, Dioxus Native auf 0.7.0;
Blitz 0.2 wird transitiv eingebunden. `Cargo.lock` gehört zum Projekt.
Grundlage der Protokollimplementierung ist
[Smithay Smallvil](https://github.com/Smithay/smithay/tree/76118ad7796e3edc46875f44c437485996a8e39d/smallvil).
Die GUI basiert auf [Dioxus Native / Blitz](https://github.com/DioxusLabs/blitz).
Lizenzhinweise stehen in [THIRD_PARTY.md](THIRD_PARTY.md).

## Diagnose und mehrere Instanzen

```sh
./target/debug/wcctl status
./target/debug/wcctl left
./target/debug/wcctl right
./target/debug/wcctl maximize
./target/debug/wcctl fullscreen
./target/debug/wcctl restore
./target/debug/wcctl cycle
./target/debug/wcctl close
./target/debug/wcctl apply /absoluter/pfad/settings.toml
```

`status` liefert Einstellungen, Wayland-Socket sowie Fensterpositionen,
Größen und Fokuszustand als JSON. Fensteraktionen richten sich an das
aktuell fokussierte Fenster; `cycle` wechselt zum nächsten.

Der lokale Steuersocket liegt in `$XDG_RUNTIME_DIR/wc-control.sock`, Modus
`0600`. `WC_SOCKET=/absoluter/pfad.sock` wählt eine andere Instanz, sowohl
für Server als auch GUI und `wcctl`. Eine zweite Instanz mit demselben Socket
wird abgelehnt. Nach einem Absturz muss ein verwaister Socket entfernt werden;
vorher prüfen, dass die betreffende Instanz wirklich beendet ist.

`RUST_LOG=info` aktiviert ausführlichere Logs. Der Server verändert die
`WAYLAND_DISPLAY`-Variable der äußeren Sitzung nicht; nur gestartete Clients
erhalten seinen eigenen Socket.

## Prüfen

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/smoke.py
WC_SMOKE_GPU=1 python3 scripts/smoke.py
```

Unit-Tests prüfen Konfigurationsvalidierung, widersprüchliche Tastenkürzel,
Geometrie bei ungeraden Auflösungen und atomare Speicherung.
Der Smoke-Test benötigt einen laufenden Desktop, startet echte Wayland-Clients
mit isolierter Konfiguration und prüft Halbierung, Maximieren, Vollbild,
Wiederherstellen, Fokuswechsel, Schließen, Live-Änderungen, Persistenz über einen
Neustart sowie fehlerhafte/überlange IPC-Nachrichten. Mit `WC_SMOKE_GPU=1`
startet zusätzlich die native Blitz-GUI innerhalb des Compositors.
Alle vom Test gestarteten Prozesse werden anschließend beendet.

## Noch offen

Für einen täglichen Desktop fehlen insbesondere DRM/KMS mit Session-/VT-
Verwaltung, mehrere physische Monitore, Workspaces, Minimieren und Taskleiste,
XWayland, Layer-Shell, Portale/Screen-Sharing, Session-Lock, IME, vollständige
Popup-Grabs, Serverdekorationen, Effekte und Fensterregeln.
Touch, Tablet und anspruchsvolle Drag-and-drop-Abläufe sind nicht fertig.
GPU-Import ist auf der vorhandenen Maschine getestet, nicht auf allen Treibern.
Der Renderer zeichnet momentan konservativ vollständig neu.
WC ist daher eine erweiterbare, tatsächlich ausführbare Basis und noch keine
vollständige Desktop-Sitzung für die Anmeldung.
