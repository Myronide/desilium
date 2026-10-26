
# Desilium — Technology Stack & Architecture

> A modern, high-performance, modular Linux Desktop Environment built with Rust and Wayland.

## 1. Project Overview

**Project:** Desilium  
**Type:** Linux Desktop Environment  
**Primary Language:** Rust  
**Display Protocol:** Wayland  
**Primary Distribution:** Myronide OS  
**Platform Support:** Distribution-independent Linux  

Desilium is a modern Linux desktop environment designed to provide a premium, intuitive, highly customizable, and performant desktop experience.

The project aims to combine the usability of traditional desktop operating systems with modern visual design, smooth animations, extensive customization, and efficient resource management.

Desilium is developed independently of Myronide OS and must remain compatible with other Linux distributions.

### Core Principles

- Native performance
- Modern and consistent design
- Zero-configuration user experience
- GUI-first system management
- Modular architecture
- High customizability
- Hardware acceleration
- Low resource overhead
- Reliability and fault isolation
- Distribution independence
- Accessibility
- Security by design

---

## 2. Technology Stack

### Core

| Component | Technology |
|---|---|
| Programming Language | Rust |
| Build System | Cargo |
| Async Runtime | Tokio |
| Serialization | Serde |
| Configuration Storage | TOML |
| Logging & Diagnostics | tracing |

### Display & Window Management

| Component | Technology |
|---|---|
| Display Protocol | Wayland |
| Compositor Framework | Smithay |
| Compositor Rendering | Smithay Renderer |
| Window Management | Custom Rust implementation |
| Desktop Protocols | Wayland Protocols / XDG |
| X11 Compatibility | XWayland |
| Input Management | libinput |
| Display Management | DRM/KMS |
| Session Management | systemd-logind |

### User Interface & Rendering

| Component | Technology |
|---|---|
| Native UI Engine | Blitz |
| Reactive UI Layer | Dioxus Native (experimental) |
| GPU Rendering | Vello / wgpu |
| Animation System | Custom Rust implementation |
| Accessibility | AccessKit |
| Icons & Assets | SVG / Vector Assets |
| Theme Engine | Custom Desilium implementation |

Blitz is responsible for native HTML/CSS rendering. Its internal dependencies, such as Stylo and Taffy, do not need to be maintained as separate architectural components.

The UI rendering backend must remain replaceable because Blitz and its native integration are still evolving.

Dioxus Native is experimental and must be evaluated before becoming a mandatory dependency.

### System Integration

| Component | Technology |
|---|---|
| IPC | zbus / D-Bus |
| Audio | PipeWire |
| Audio Session Management | WirePlumber |
| Networking | NetworkManager |
| Bluetooth | BlueZ |
| Power Management | UPower |
| Power Profiles | power-profiles-daemon |
| Notifications | Custom notification service |
| Desktop Portals | xdg-desktop-portal integration |
| Secrets & Credentials | Secret Service API integration |

System integrations should reuse established Linux services rather than unnecessarily reimplementing their functionality.

---

## 3. Zero-Configuration Philosophy

### Zero-Config. GUI-First. Out of the Box.

**Desilium must never require users to manually edit configuration files to use, customize, maintain, or troubleshoot the desktop environment.**

This is a mandatory architectural and product requirement, not an optional feature.

Configuration files may exist internally, but they must never become a prerequisite for ordinary user interaction.

### Requirements

1. Every user-facing configuration option must be available through a graphical interface.
2. Desilium must provide sensible default settings.
3. Hardware must be automatically detected and configured wherever technically possible.
4. Configuration changes should take effect immediately whenever possible.
5. Users must not need to restart the desktop to apply ordinary settings.
6. All configuration changes must be validated before being applied.
7. Invalid configurations must not prevent Desilium from starting.
8. Failed changes must be reversible.
9. Components must provide graphical reset and recovery options.
10. Settings must be discoverable through a unified search interface.
11. Advanced customization must remain possible without editing files.
12. User-facing errors must provide meaningful explanations and recovery actions.

### Automatic Configuration

Desilium should automatically handle:

- Connected monitors
- Display resolution and refresh rates
- Display scaling
- Audio devices
- Input devices
- Keyboard layouts
- Network status
- Bluetooth availability
- Power capabilities
- GPU capabilities
- Default desktop configuration
- Theme initialization
- Session restoration

Automatic detection does not mean making irreversible decisions without user consent.

### Graphical Configuration

The settings application must expose controls for:

- Appearance and themes
- Accent colors
- Typography
- Icons
- Wallpapers
- Window decorations
- Desktop effects
- Animations
- Taskbar and panels
- Start menu
- Widgets
- Virtual desktops
- Window management
- Keyboard shortcuts
- Mouse and touchpad
- Displays
- Audio
- Networking
- Bluetooth
- Power management
- Notifications
- Accessibility
- Privacy and security
- Startup applications
- Session behavior

The settings interface must support both simple controls and advanced configuration without requiring terminal usage.

---

## 4. Configuration Architecture

Desilium uses a centralized configuration service.

### Responsibilities

- Configuration loading
- Schema validation
- Default generation
- Configuration migration
- Change notifications
- Transactional updates
- Automatic backups
- Recovery and rollback
- Conflict handling
- Component synchronization

### Configuration Flow

```text
Desilium Settings
        |
        v
Configuration API
        |
        v
Validation & Transactions
        |
        v
Configuration Service
        |
        +----> Persistent Storage
        |
        +----> Change Notifications
                     |
                     +----> Compositor
                     +----> Desktop Shell
                     +----> Theme Engine
                     +----> System Services
```

### Configuration Rules

- Configuration is managed through typed Rust APIs.
- Every setting must have a documented default value.
- Configuration changes must be validated.
- Components must react to configuration changes.
- Settings should not require process restarts.
- Persistent writes should be atomic.
- Schema migrations must preserve existing preferences.
- Corrupted configuration must trigger safe recovery.
- Configuration backups must support rollback.
- External edits, when supported, must be detected safely.
- Sensitive values must not be stored as plain text in TOML.

TOML is an internal storage format, not a required user interface.

---

## 5. Modular Architecture

Desilium consists of independently maintainable components.

### Main Components

| Component | Responsibility |
|---|---|
| desilium-compositor | Wayland compositor and window management |
| desilium-shell | Desktop interface and panels |
| desilium-ui | UI rendering abstraction |
| desilium-settings | Graphical system configuration |
| desilium-files (Doria) | Native file manager, drives, pinned places and network locations |
| desilium-config | Configuration service |
| desilium-theme | Themes and visual customization |
| desilium-session | Desktop session lifecycle |
| desilium-notifications | Notification service |
| desilium-lock | Lock screen |
| desilium-launcher | Application launcher |
| desilium-widgets | Widget management |
| desilium-portal | Desktop portal integration |
| desilium-diagnostics | Diagnostics and recovery |

The exact crate and process boundaries should be determined according to reliability, performance, and security requirements.

### Architecture Rules

- The compositor must remain operational independently of the desktop shell.
- A shell crash must not terminate the desktop session.
- Individual components must be independently restartable where practical.
- System services must communicate through stable interfaces.
- Rendering and business logic must remain separated.
- UI implementations must be replaceable.
- Components must avoid unnecessary runtime dependencies.
- Critical system functionality must not depend on optional visual features.
- Failure recovery must be automatic whenever safe.

---

## 6. Desktop Experience

Desilium should provide a polished desktop experience with familiar interaction patterns while maintaining its own visual identity.

### Visual Design

- Modern design language
- Consistent spacing and typography
- Smooth transitions
- Hardware-accelerated effects
- Optional transparency and blur
- Consistent window decorations
- Adaptive scaling
- Light and dark themes
- Configurable accent colors
- Customizable animation behavior

### Desktop Functionality

- Taskbar and panels
- Application launcher
- System tray
- Notifications
- Quick settings
- Virtual desktops
- Window snapping
- Tiling and floating window management
- Multi-monitor support
- Drag and drop
- Clipboard integration
- Screen capture
- Screen sharing
- Global keyboard shortcuts
- Session restoration
- Lock screen
- Power menu

Every supported desktop feature must be configurable through graphical interfaces where configuration is relevant.

---

## 7. Performance Requirements

Desilium must prioritize responsive interaction and efficient resource utilization.

### Principles

- Avoid unnecessary background processes.
- Minimize idle CPU usage.
- Avoid unnecessary memory allocations.
- Prefer event-driven processing.
- Use GPU acceleration appropriately.
- Avoid unnecessary CPU-to-GPU copies.
- Minimize rendering work when the desktop is idle.
- Cache reusable rendering resources.
- Keep animations independent from blocking system operations.
- Prevent expensive system operations from blocking the UI.
- Profile performance before implementing optimizations.

### Measurement

Performance must be evaluated through reproducible benchmarks.

Important metrics include:

- Idle RAM consumption
- Idle CPU usage
- Frame latency
- Input-to-display latency
- Window opening responsiveness
- Animation frame consistency
- Shell startup time
- Session startup time
- GPU memory consumption
- Multi-monitor performance

Performance targets should be established after an initial functional prototype.

---

## 8. Security & Reliability

### Security

- Respect Wayland security boundaries.
- Use established authentication mechanisms.
- Avoid privileged UI processes.
- Separate privileged operations from unprivileged interfaces.
- Validate IPC messages and configuration updates.
- Require authorization for privileged actions.
- Use desktop portals for supported sandboxed workflows.
- Avoid exposing sensitive information through logs.
- Keep credentials outside ordinary configuration files.

### Reliability

- Graceful component restarts
- Automatic crash recovery
- Configuration rollback
- Safe defaults
- Session recovery
- Diagnostic reporting
- Structured logging
- Robust error handling
- Controlled shutdown behavior

The system must remain usable even when noncritical desktop components fail.

---

## 9. Development Strategy

### Phase 1 — Foundation

- Initialize Cargo workspace.
- Define architecture and interfaces.
- Integrate Smithay.
- Establish a minimal Wayland compositor.
- Build a basic window management system.
- Create a nested development environment.
- Establish automated testing and logging.

### Phase 2 — UI Foundation

- Evaluate Blitz.
- Develop the rendering abstraction.
- Prototype desktop panels.
- Implement the application launcher.
- Establish the theme system.
- Validate input handling and rendering performance.

### Phase 3 — Configuration Platform

- Implement the centralized configuration service.
- Define typed setting schemas.
- Implement validation and transactions.
- Implement automatic defaults.
- Add live change notifications.
- Implement graphical settings.
- Add rollback and recovery mechanisms.

### Phase 4 — Desktop Features

- Multi-monitor management
- Window snapping
- Virtual desktops
- Notifications
- System tray
- Quick settings
- Audio controls
- Network controls
- Power management
- Lock screen

### Phase 5 — Integration

- XWayland compatibility
- Desktop portals
- Screen sharing
- Screen capture
- Clipboard integration
- Drag and drop
- Accessibility
- Session management
- Application integration

### Phase 6 — Stabilization

- Performance profiling
- Memory optimization
- Crash recovery
- Configuration migration
- Hardware compatibility testing
- Security review
- Accessibility testing
- Regression testing

---

## 10. Relationship to Myronide OS

Desilium is the desktop environment.

Myronide is the Linux distribution.

The projects must remain architecturally independent.

### Desilium

Responsible for:

- Desktop interface
- Window management
- Desktop effects
- User interaction
- Desktop customization
- Session experience

### Myronide

Responsible for:

- Linux distribution integration
- Package management
- System updates
- Installation
- Default system configuration
- Distribution-specific services
- Recovery infrastructure

Desilium must not contain hardcoded assumptions that require Myronide.

Myronide may ship Desilium as its default desktop environment.

---

## 11. Definition of Done

A Desilium feature is not considered complete unless:

- It works reliably.
- It respects the modular architecture.
- It provides sensible defaults.
- It is configurable through the GUI where applicable.
- It does not require manual configuration editing.
- It handles invalid input.
- It provides meaningful feedback.
- It supports safe recovery where necessary.
- It has appropriate automated tests.
- Its performance has been evaluated.

### Non-Negotiable Principle

**Users must never be required to manually modify configuration files to use, personalize, maintain, or troubleshoot Desilium.**

Desilium must combine the customizability of Linux with the accessibility and polish expected from a modern consumer operating system.
