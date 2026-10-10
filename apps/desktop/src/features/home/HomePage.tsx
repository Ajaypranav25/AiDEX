import type { PageId } from "../../lib/navigation";

type Props = { activeLabel: string; onNavigate: (page: PageId) => void };

const principles = [
  { number: "01", title: "Portable by design", detail: "Keep your agent environment and durable state with the drive." },
  { number: "02", title: "Local-first runtime", detail: "Bring local models and tools; use cloud providers when you choose." },
  { number: "03", title: "Human-controlled", detail: "Mediate sensitive actions through explicit policy and approval." },
];

export function HomePage({ activeLabel, onNavigate }: Props) {
  return (
    <section className="page-content" aria-labelledby="page-title">
      <div className="eyebrow"><span className="eyebrow-line" /> YOUR ENVIRONMENT, READY TO GO</div>
      <div className="hero">
        <div className="hero-copy">
          <h1 id="page-title">A workspace that<br /><span>travels with you.</span></h1>
          <p>AiDEX brings your AI development environment, agents, tools, and projects together in one portable workspace.</p>
          <button className="primary-button" onClick={() => onNavigate("workspace")}>Open workspace <span aria-hidden="true">↗</span></button>
        </div>
        <div className="hero-art" aria-hidden="true"><div className="orbit orbit-one" /><div className="orbit orbit-two" /><div className="orbit orbit-three" /><div className="core-mark">A<span>i</span></div><span className="orbit-node node-one" /><span className="orbit-node node-two" /><span className="orbit-node node-three" /></div>
      </div>
      <div className="section-heading"><div><span className="eyebrow">THE AiDEX APPROACH</span><h2>Built for work that moves.</h2></div><span className="page-context">{activeLabel}</span></div>
      <div className="principle-grid">{principles.map(({ number, title, detail }) => <article className="principle-card" key={number}><span className="card-number">{number}</span><h3>{title}</h3><p>{detail}</p></article>)}</div>
      <div className="connection-note"><span className="note-icon">i</span><p><strong>Desktop shell is ready.</strong> The AiDEX Core API is not connected yet. This interface does not run agents or simulate backend activity.</p></div>
    </section>
  );
}
