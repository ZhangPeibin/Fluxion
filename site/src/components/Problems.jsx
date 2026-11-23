import Reveal from "./Reveal.jsx";
import SectionHeading from "./SectionHeading.jsx";

const ICONS = [
  <svg key="grid" viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
    <path d="M4 7h7M4 12h13M4 17h9" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    <circle cx="18" cy="7" r="2.1" fill="none" stroke="currentColor" strokeWidth="1.7" />
    <circle cx="17" cy="17" r="2.1" fill="none" stroke="currentColor" strokeWidth="1.7" />
  </svg>,
  <svg key="shield" viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
    <path d="M12 3.5 19 6v5.4c0 4.3-2.9 7.4-7 9.1-4.1-1.7-7-4.8-7-9.1V6l7-2.5Z" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
    <path d="M9.2 12.2 11.4 14.4 15 10.6" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
  </svg>,
];

export default function Problems({ t }) {
  return (
    <section className="section problems" id="problems">
      <div className="container">
        <SectionHeading eyebrow={t.problems.eyebrow} title={t.problems.title} />

        <div className="problem-grid">
          {t.problems.items.map((item, index) => (
            <Reveal className="problem-card" key={item.title} delay={index * 90}>
              <span className="problem-index">0{index + 1}</span>
              <span className="problem-icon">{ICONS[index]}</span>
              <h3>{item.title}</h3>
              <p>{item.body}</p>
              <ul className="tick-list">
                {item.points.map((point) => (
                  <li key={point}>{point}</li>
                ))}
              </ul>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
