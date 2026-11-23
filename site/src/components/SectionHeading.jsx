import Reveal from "./Reveal.jsx";

export default function SectionHeading({ eyebrow, title, lead, className = "" }) {
  return (
    <div className={`section-heading${className ? ` ${className}` : ""}`}>
      <Reveal as="p" className="eyebrow">
        <span className="eyebrow-line" aria-hidden="true" />
        {eyebrow}
      </Reveal>
      <Reveal as="h2" delay={60}>
        {title}
      </Reveal>
      {lead && (
        <Reveal as="p" className="section-lead" delay={120}>
          {lead}
        </Reveal>
      )}
    </div>
  );
}
