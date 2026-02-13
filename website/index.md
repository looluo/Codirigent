---
layout: default
title: Codirigent - AI Coding Agent Orchestration IDE
---

<!-- Hero Section -->
<section class="hero">
  <img src="../assets/icons/app-icon.svg" alt="Codirigent" class="hero-logo">

  <h1>AI Coding Agent Orchestration IDE</h1>

  <p>
    Terminal-based development environment with clipboard integration and session management
  </p>

  {% include download-buttons.html %}

  <div class="scroll-indicator">↓</div>
</section>

<!-- Performance Comparison Section -->
<section class="performance-section">
  <div class="container">
    <div class="section-header">
      <h2>Built for Performance</h2>
      <p>Lightweight terminal IDE vs heavyweight Electron apps</p>
    </div>

    <div class="comparison-grid">
      <div class="metric-row">
        <div class="metric-label">Memory Usage</div>
        <div class="metric-bar">
          <div class="bar">
            <div class="bar-fill codirigent" style="width: 15%;"></div>
          </div>
          <span class="metric-value">~120 MB</span>
        </div>
        <div class="metric-bar">
          <div class="bar">
            <div class="bar-fill vscode" style="width: 100%;"></div>
          </div>
          <span class="metric-value">~800 MB</span>
        </div>
      </div>

      <div class="metric-row">
        <div class="metric-label">CPU Usage</div>
        <div class="metric-bar">
          <div class="bar">
            <div class="bar-fill codirigent" style="width: 13%;"></div>
          </div>
          <span class="metric-value">~2%</span>
        </div>
        <div class="metric-bar">
          <div class="bar">
            <div class="bar-fill vscode" style="width: 100%;"></div>
          </div>
          <span class="metric-value">~15%</span>
        </div>
      </div>

      <div class="metric-row">
        <div class="metric-label">Startup Time</div>
        <div class="metric-bar">
          <div class="bar">
            <div class="bar-fill codirigent" style="width: 3%;"></div>
          </div>
          <span class="metric-value">&lt;0.5s</span>
        </div>
        <div class="metric-bar">
          <div class="bar">
            <div class="bar-fill vscode" style="width: 100%;"></div>
          </div>
          <span class="metric-value">~3s</span>
        </div>
      </div>

      <div class="metric-row">
        <div class="metric-label">Disk Space</div>
        <div class="metric-bar">
          <div class="bar">
            <div class="bar-fill codirigent" style="width: 10%;"></div>
          </div>
          <span class="metric-value">~50 MB</span>
        </div>
        <div class="metric-bar">
          <div class="bar">
            <div class="bar-fill vscode" style="width: 100%;"></div>
          </div>
          <span class="metric-value">~500 MB</span>
        </div>
      </div>

      <p class="metric-note">
        * Placeholder metrics - Add real benchmarks after testing
      </p>
    </div>
  </div>
</section>

<!-- Screenshots Section -->
<section class="screenshots-section">
  <div class="container">
    <div class="section-header">
      <h2>See It In Action</h2>
      <p>Terminal-based IDE designed for AI-powered development</p>
    </div>

    <div class="screenshots-grid">
      <div class="screenshot-card">
        <div class="screenshot-placeholder">
          Screenshot: Terminal with file tree navigation
        </div>
        <div class="screenshot-caption">Terminal with file tree navigation</div>
      </div>

      <div class="screenshot-card">
        <div class="screenshot-placeholder">
          Screenshot: Session management in action
        </div>
        <div class="screenshot-caption">Session management in action</div>
      </div>

      <div class="screenshot-card">
        <div class="screenshot-placeholder">
          Screenshot: Clipboard integration workflow
        </div>
        <div class="screenshot-caption">Clipboard integration workflow</div>
      </div>
    </div>
  </div>
</section>

<!-- Installation Section -->
<section class="installation-section">
  <div class="container">
    <div class="section-header">
      <h2>Getting Started</h2>
      <p>Quick installation guide for Windows and macOS</p>
    </div>

    <div class="installation-grid">
      <div class="install-card">
        <h3>🪟 Windows</h3>
        <ol class="install-steps">
          <li>Download <code>codirigent-v0.1.0-x86_64-pc-windows-msvc.msi</code></li>
          <li>Run the installer</li>
          <li>Launch from Start Menu or run <code>codirigent</code> in terminal</li>
        </ol>
      </div>

      <div class="install-card">
        <h3>🍎 macOS</h3>
        <ol class="install-steps">
          <li>Download <code>codirigent-v0.1.0-aarch64-apple-darwin.dmg</code></li>
          <li>Open the DMG and drag Codirigent to Applications</li>
          <li>Launch from Applications or run <code>codirigent</code> in terminal</li>
        </ol>
      </div>
    </div>

    <div class="requirements">
      <h4>System Requirements</h4>
      <ul>
        <li><strong>Windows:</strong> Windows 10 or later</li>
        <li><strong>macOS:</strong> macOS 13.0 (Ventura) or later</li>
        <li><strong>Rust:</strong> For building from source (stable channel)</li>
      </ul>
    </div>

    <div class="requirements">
      <h4>Build from Source</h4>
      <pre><code>git clone https://github.com/your-username/codirigent
cd codirigent
cargo build --release</code></pre>
    </div>
  </div>
</section>
