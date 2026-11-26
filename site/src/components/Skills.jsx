import { useEffect, useRef, useState } from "react";

import CodeBlock from "./CodeBlock.jsx";
import Reveal from "./Reveal.jsx";
import SectionHeading from "./SectionHeading.jsx";
import { GITHUB_URL, SKILLS_COMMAND } from "../i18n.js";

function PromptRotator({ prompts, hint, copyLabel, copiedLabel }) {
  const [index, setIndex] = useState(0);
  const [paused, setPaused] = useState(false);
  const [copied, setCopied] = useState(-1);
  const timer = useRef(null);

  useEffect(() => {
    if (paused) return undefined;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return undefined;
    const id = setInterval(() => setIndex((value) => (value + 1) % prompts.length), 4200);
    return () => clearInterval(id);
  }, [paused, prompts.length]);

  useEffect(() => () => clearTimeout(timer.current), []);

  async function copyPrompt(event, prompt, position) {
    event.stopPropagation();
    try {
      await navigator.clipboard.writeText(prompt);
      setCopied(position);
      setIndex(position);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(-1), 1600);
    } catch {
      setCopied(-1);
    }
  }

  return (
    <div
      className="prompt-card"
      onPointerEnter={() => setPaused(true)}
      onPointerLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
    >
      <div className="prompt-bar">
        <span className="prompt-dots" aria-hidden="true">
          <i />
          <i />
          <i />
        </span>
        <span className="prompt-hint">{hint}</span>
      </div>

      <div className="prompt-stage" aria-live="polite">
        {prompts.map((prompt, position) => (
          <p
            key={prompt}
            className={`prompt-line${position === index ? " is-active" : ""}`}
            aria-hidden={position !== index}
          >
            <span className="prompt-prefix" aria-hidden="true">›</span>
            {prompt}
          </p>
        ))}
      </div>

      <div className="prompt-foot">
        <div className="prompt-indicators" role="tablist" aria-label={hint}>
          {prompts.map((prompt, position) => (
            <button
              key={prompt}
              type="button"
              role="tab"
              aria-selected={position === index}
              aria-label={`${position + 1} / ${prompts.length}`}
              className={`prompt-indicator${position === index ? " is-active" : ""}`}
              onClick={() => setIndex(position)}
            />
          ))}
        </div>
        <button
          type="button"
          className="prompt-copy"
          onClick={(event) => copyPrompt(event, prompts[index], index)}
        >
          {copied === index ? copiedLabel : copyLabel}
        </button>
      </div>
    </div>
  );
}

export default function Skills({ t }) {
  return (
    <section className="section skills" id="skills">
      <div className="container">
        <SectionHeading eyebrow={t.skills.eyebrow} title={t.skills.title} lead={t.skills.lead} />

        <div className="skills-top">
          <Reveal className="skills-config">
            <h3>{t.skills.configLabel}</h3>
            <CodeBlock
              code={SKILLS_COMMAND}
              path="Terminal"
              copyLabel={t.captions.copy}
              copiedLabel={t.captions.copied}
            />
          </Reveal>

          <Reveal className="skills-prompts" delay={90}>
            <h3>{t.skills.promptTitle}</h3>
            <PromptRotator
              prompts={t.skills.prompts}
              hint={t.skills.promptHint}
              copyLabel={t.captions.copy}
              copiedLabel={t.captions.copied}
            />
            <a
              className="prompt-more"
              href={`${GITHUB_URL}/blob/master/docs/AGENT-EXAMPLES.md`}
              target="_blank"
              rel="noreferrer"
            >
              {t.skills.examplesLink}
              <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                <path d="M4 12 12 4M6 4h6v6" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </a>
          </Reveal>
        </div>

        <Reveal as="h3" className="skills-list-title" delay={60}>
          {t.skills.listTitle}
        </Reveal>

        <div className="skill-grid">
          {t.skillsList.map((skill, index) => (
            <Reveal as="a" className="skill-card" key={skill.name} delay={index * 50}
              href={`${GITHUB_URL}/blob/master/skills/${skill.name}/SKILL.md`}
              target="_blank"
              rel="noreferrer"
            >
              <span className="skill-slash" aria-hidden="true">/</span>
              <span className="skill-name">{skill.name}</span>
              <span className="skill-desc">{skill.desc}</span>
              <span className="skill-link">
                {t.captions.skillLink}
                <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                  <path d="M4 12 12 4M6 4h6v6" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </span>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
