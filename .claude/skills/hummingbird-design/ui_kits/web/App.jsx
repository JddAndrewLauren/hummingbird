const { Button, IconButton, Badge, Icon } = window.HummingbirdDesignSystem_dcdceb;

const TITLES = { now: "What's next", triage: "Triage", routes: "Routes", alerts: "Alerts", settings: "Settings" };

function WebApp() {
  const [screen, setScreen] = React.useState("now");
  const [theme, setTheme] = React.useState("light");
  React.useEffect(() => { document.documentElement.setAttribute("data-theme", theme); }, [theme]);
  const data = window.HB_DATA;
  return (
    <div style={{ display: "flex", minHeight: "100vh", background: "var(--surface-page)" }}>
      <NavRail screen={screen} onScreen={setScreen} theme={theme} onTheme={() => setTheme(theme === "dark" ? "light" : "dark")} />
      <main style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
        <header style={{ display: "flex", alignItems: "center", gap: "var(--space-5)", padding: "var(--space-7) var(--gutter-page) var(--space-6)" }}>
          <h1 style={{ font: "var(--type-h1)", letterSpacing: "var(--tracking-heading)", flex: 1 }}>{TITLES[screen]}</h1>
          <Badge mono tone="neutral">synced · 0 queued</Badge>
          <IconButton icon="search" label="Search" />
          <IconButton icon="refresh-cw" label="Refresh" />
          <Button iconLeft="feather" onClick={() => setScreen("triage")}>Capture</Button>
        </header>
        <div style={{ flex: 1, padding: "0 var(--gutter-page) var(--space-11)" }}>
          {screen === "now" && <NowScreen data={data} onOpen={setScreen} />}
          {screen === "triage" && <TriageScreen data={data} />}
          {screen === "routes" && <RouteScreen data={data} />}
          {screen === "alerts" && <AlertsScreen data={data} />}
          {screen === "settings" && <SettingsScreen data={data} />}
        </div>
      </main>
    </div>
  );
}
ReactDOM.createRoot(document.getElementById("root")).render(<WebApp />);
