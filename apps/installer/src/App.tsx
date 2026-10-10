const stages = [
  { number: "01", title: "Choose a drive", detail: "Select a removable drive and review available capacity.", state: "Planned" },
  { number: "02", title: "Set up workspace", detail: "Choose where portable settings and project state will live.", state: "Planned" },
  { number: "03", title: "Choose models", detail: "Select local model packs suited to your drive and host.", state: "Planned" },
  { number: "04", title: "Choose agents", detail: "Configure the agent runtimes you want to use.", state: "Planned" },
  { number: "05", title: "Protect your data", detail: "Set up encryption and recovery for private workspace data.", state: "Not available" },
  { number: "06", title: "Review and prepare", detail: "Review the plan before any changes are made to a drive.", state: "Planned" },
  { number: "07", title: "Finish setup", detail: "Confirm readiness and learn how to launch AiDEX Desktop.", state: "Planned" },
];

export default function App() {
  return (
    <main className="shell">
      <header className="topbar">
        <a className="brand" href="#welcome" aria-label="AiDEX Setup home">
          <span className="brand-mark">A</span><span>AiDEX <span className="brand-muted">/ SETUP</span></span>
        </a>
        <span className="edition"><i /> PORTABLE WORKSPACE</span>
      </header>

      <section className="hero" id="welcome">
        <div className="hero-copy">
          <div className="eyebrow"><span /> YOUR WORKSPACE, READY TO TRAVEL</div>
          <h1>Take your AI<br />workspace <em>with you.</em></h1>
          <p className="intro">AiDEX is designed to bring your models, agents, and workspace together on a portable drive. This preview walks through the setup we’re building.</p>
          <div className="notice"><span className="notice-icon">i</span><p><strong>Setup is a preview.</strong> This app does not scan, format, encrypt, or write to drives. No installation actions are available yet.</p></div>
        </div>
        <div className="drive-art" aria-hidden="true">
          <div className="orbit orbit-one" /><div className="orbit orbit-two" />
          <div className="drive"><div className="drive-head"><span className="drive-logo">A</span><span className="drive-light" /></div><div className="drive-label">AIDEX<span>PORTABLE SYSTEM</span></div><div className="drive-port" /></div>
          <span className="float-tag tag-model">◈ &nbsp; MODELS</span><span className="float-tag tag-agent">✳ &nbsp; AGENTS</span><span className="float-tag tag-vault">⌑ &nbsp; YOUR DATA</span>
        </div>
      </section>

      <section className="journey" aria-labelledby="journey-title">
        <div className="section-heading"><div><div className="eyebrow">THE SETUP JOURNEY</div><h2 id="journey-title">A clear path to portable.</h2></div><span className="preview-pill">PREVIEW · NO ACTIONS ENABLED</span></div>
        <div className="stage-list">
          {stages.map((stage) => <article className="stage" key={stage.number}>
            <span className="stage-number">{stage.number}</span>
            <div className="stage-copy"><h3>{stage.title}</h3><p>{stage.detail}</p></div>
            <span className={`stage-state ${stage.state === "Not available" ? "unavailable" : ""}`}><span />{stage.state}</span>
          </article>)}
        </div>
        <div className="footer-note"><span className="lock">⌑</span><span>Nothing will be changed on your computer or a drive by this preview.</span></div>
      </section>
      <footer><span>AiDEX SETUP <span className="footer-dot">·</span> PORTABLE BY DESIGN</span><span>Setup capabilities are still in development.</span></footer>
    </main>
  );
}
