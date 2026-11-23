import { useEffect, useRef, useState } from "react";

async function writeClipboard(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const area = document.createElement("textarea");
      area.value = text;
      area.setAttribute("readonly", "");
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.appendChild(area);
      area.select();
      const copied = document.execCommand("copy");
      document.body.removeChild(area);
      return copied;
    } catch {
      return false;
    }
  }
}

export default function CopyButton({ text, label, copiedLabel, className = "" }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef(null);

  useEffect(() => () => clearTimeout(timer.current), []);

  async function handleClick() {
    const ok = await writeClipboard(text);
    if (!ok) return;
    setCopied(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), 1600);
  }

  return (
    <button
      type="button"
      className={`copy-button${copied ? " is-copied" : ""}${className ? ` ${className}` : ""}`}
      onClick={handleClick}
      aria-live="polite"
    >
      <span className="copy-icon" aria-hidden="true">
        {copied ? (
          <svg viewBox="0 0 16 16" width="14" height="14">
            <path d="M3.5 8.5l3 3 6-6.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        ) : (
          <svg viewBox="0 0 16 16" width="14" height="14">
            <rect x="5.5" y="5.5" width="8" height="8" rx="1.6" fill="none" stroke="currentColor" strokeWidth="1.3" />
            <path d="M10.5 3.5H4.1A1.6 1.6 0 0 0 2.5 5.1v6.4" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
          </svg>
        )}
      </span>
      <span>{copied ? copiedLabel ?? "Copied" : label ?? "Copy"}</span>
    </button>
  );
}
