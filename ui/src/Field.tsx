import { useEffect, useState } from "react";
export interface Choices {
  choices?: readonly (readonly [string, string | number])[];
  list?: string;
  type?: string;
}
export function Field<T extends string | number | undefined>({
  label,
  value,
  change,
  choices,
  list,
  type = "text",
}: { label: string; value: T; change: (v: NonNullable<T>) => void } & Choices) {
  const [text, setText] = useState(String(value ?? ""));
  useEffect(() => {
    setText(String(value ?? ""));
  }, [value]);
  const edit = (raw: string) => {
    setText(raw);
    const numeric = type === "number" || typeof value === "number";
    if (numeric && (!raw || !Number.isFinite(Number(raw)))) return;
    change((numeric ? Number(raw) : raw) as NonNullable<T>);
  };
  return (
    <label>
      {label}
      {choices ? (
        <select
          aria-label={label}
          value={String(value ?? "")}
          onChange={(e) => edit(e.target.value)}
        >
          {!choices.some((c) => String(c[1]) === String(value)) && (
            <option>{value}</option>
          )}
          {choices.map(([text, val]) => (
            <option key={val} value={val}>
              {text}
            </option>
          ))}
        </select>
      ) : type === "textarea" ? (
        <textarea
          aria-label={label}
          value={text}
          onChange={(e) => edit(e.target.value)}
        />
      ) : (
        <input
          aria-label={label}
          type={type}
          step={type === "number" ? "any" : undefined}
          list={list}
          value={text}
          onChange={(e) => edit(e.target.value)}
        />
      )}
    </label>
  );
}
