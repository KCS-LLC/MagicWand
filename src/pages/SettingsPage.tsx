interface SettingsPageProps {
  pollInterval: number;
  onPollIntervalChange: (value: number) => void;
}

export function SettingsPage({ pollInterval, onPollIntervalChange }: SettingsPageProps) {
  return (
    <div className="page-view">
      <header className="header">
        <h1>Settings</h1>
      </header>

      <section className="settings-section">
        <h2>Performance</h2>
        <div className="setting-row">
          <label>Value poll interval (ms)</label>
          <input
            type="number"
            min={500}
            max={10000}
            step={500}
            value={pollInterval}
            onChange={e => onPollIntervalChange(Number(e.target.value))}
          />
        </div>
      </section>

      <section className="settings-section">
        <h2>Trainers</h2>
        <div className="setting-row">
          <label>Custom trainer folder</label>
          <span className="setting-value">public/trainers/</span>
        </div>
        <p className="setting-hint">
          Drop <code>.json</code> trainer files here and add them to <code>index.json</code> to load them automatically.
        </p>
      </section>

      <section className="settings-section">
        <h2>About</h2>
        <p>Magic Wand v1.0</p>
        <p>A free, open-source game trainer built with Tauri + React.</p>
      </section>
    </div>
  );
}
