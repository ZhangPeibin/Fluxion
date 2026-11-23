import CopyButton from "./CopyButton.jsx";

export default function CodeBlock({ code, path, label, copyLabel, copiedLabel, className = "" }) {
  return (
    <div className={`codeblock${className ? ` ${className}` : ""}`}>
      <div className="codeblock-bar">
        <span className="codeblock-dots" aria-hidden="true">
          <i />
          <i />
          <i />
        </span>
        {path && <span className="codeblock-path">{path}</span>}
        <CopyButton text={code} label={copyLabel ?? label} copiedLabel={copiedLabel} />
      </div>
      <pre>
        <code>{code}</code>
      </pre>
    </div>
  );
}
