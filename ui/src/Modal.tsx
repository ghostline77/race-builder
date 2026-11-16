import { useEffect, useRef } from "react";
import { changed } from "./state";
type Field = {
  name: string;
  value: string;
  type?: "text" | "textarea" | "number";
  min?: number;
  required?: boolean;
};
type Values = Record<string, string>;
let current: {
  title: string;
  fields: Field[];
  resolve: (v: Values | null) => void;
} | null = null;
export function ask(title: string, fields: Field[]): Promise<Values | null> {
  if (current) current.resolve(null);
  return new Promise((resolve) => {
    current = { title, fields, resolve };
    changed();
  });
}
export function Modal() {
  const dialog = useRef<HTMLDialogElement>(null),
    request = current;
  useEffect(() => {
    if (request && !dialog.current?.open) dialog.current?.showModal();
  }, [request]);
  if (!request) return null;
  function finish(values: Values | null) {
    request!.resolve(values);
    current = null;
    changed();
  }
  return (
    <dialog
      ref={dialog}
      onCancel={(e) => {
        e.preventDefault();
        finish(null);
      }}
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          const values: Values = {};
          for (const [k, v] of new FormData(e.currentTarget))
            values[k] = String(v);
          finish(values);
        }}
      >
        <h2>{request.title}</h2>
        {request.fields.map((f) => (
          <label key={f.name}>
            {f.name}
            {f.type === "textarea" ? (
              <textarea
                required={f.required ?? false}
                name={f.name}
                defaultValue={f.value}
              />
            ) : (
              <input
                name={f.name}
                type={f.type ?? "text"}
                required={f.required ?? false}
                min={f.min}
                step={f.type === "number" ? "any" : undefined}
                defaultValue={f.value}
              />
            )}
          </label>
        ))}
        <div className="dialog-actions">
          <button type="button" onClick={() => finish(null)}>
            Cancel
          </button>
          <button>Save</button>
        </div>
      </form>
    </dialog>
  );
}
