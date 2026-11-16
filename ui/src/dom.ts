interface Controls {
  "#map": HTMLCanvasElement;
  "#projects": HTMLSelectElement;
  "#snap": HTMLInputElement;
  "#height": HTMLInputElement;
  "#free-z": HTMLInputElement;
  "#role": HTMLSelectElement;
  "#show-samples": HTMLInputElement;
  "#mark-filter": HTMLInputElement;
  "#looped": HTMLInputElement;
  "#force-player": HTMLInputElement;
  "#file": HTMLInputElement;
  "#selected-details": HTMLDetailsElement;
  "#dialog": HTMLDialogElement;
  "#undo": HTMLButtonElement;
  "#redo": HTMLButtonElement;
  "#build": HTMLButtonElement;
  "#finish-drawing": HTMLButtonElement;
}
export function $<K extends keyof Controls>(selector: K): Controls[K];
export function $(selector: string): HTMLElement;
export function $(selector: string): HTMLElement {
  const node = document.querySelector<HTMLElement>(selector);
  if (!node) throw Error(`Missing UI element: ${selector}`);
  return node;
}
export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  text?: string,
  cls?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (cls) node.className = cls;
  return node;
}
export const errorMessage = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
export function safely<Args extends unknown[]>(fn: (...args: Args) => unknown) {
  return async (...args: Args) => {
    try {
      return await fn(...args);
    } catch (error) {
      notice(errorMessage(error), true);
      return undefined;
    }
  };
}
export function on<K extends keyof HTMLElementEventMap>(
  selector: string,
  event: K,
  fn: (event: HTMLElementEventMap[K]) => unknown,
) {
  $(selector).addEventListener(event, safely(fn));
}
export function notice(text: string, bad = false) {
  $("#notice").textContent = text;
  $("#notice").classList.toggle("error", bad);
  $("#notice").hidden = false;
}
export interface InputOptions {
  type?: string;
  choices?: readonly (readonly [string, string | number])[];
  list?: string;
}
export function createInput(beforeEdit: () => void, afterEdit: () => void) {
  return function input<T extends string | number | undefined>(
    parent: HTMLElement,
    label: string,
    value: T,
    change: (value: NonNullable<T>) => void,
    { type = "text", choices, list }: InputOptions = {},
  ) {
    const wrap = el("label", label),
      node = choices
        ? el("select")
        : type === "textarea"
          ? el("textarea")
          : el("input");
    if (choices) {
      for (const [text, val] of choices)
        node.append(new Option(text, String(val)));
      if (!choices.some((c) => String(c[1]) === String(value)))
        node.append(new Option(String(value ?? ""), String(value ?? "")));
    } else if (node instanceof HTMLInputElement) node.type = type;
    node.value = String(value ?? "");
    node.setAttribute("aria-label", label);
    if (type === "number" && node instanceof HTMLInputElement)
      node.step = "any";
    if (list) node.setAttribute("list", list);
    node.addEventListener(choices ? "change" : "input", () => {
      if (
        type === "number" &&
        (!node.value || !Number.isFinite(Number(node.value)))
      )
        return;
      beforeEdit();
      change(
        (type === "number" || typeof value === "number"
          ? Number(node.value)
          : node.value) as NonNullable<T>,
      );
      afterEdit();
    });
    wrap.append(node);
    parent.append(wrap);
    return node;
  };
}
export function button(
  parent: HTMLElement,
  text: string,
  fn: () => unknown,
  cls = "quiet",
) {
  const node = el("button", text, cls);
  node.type = "button";
  node.onclick = safely(fn);
  parent.append(node);
  return node;
}
export async function dialog<T>(
  title: string,
  build: (parent: HTMLElement) => () => T,
): Promise<T | null> {
  const body = $("#dialog-content");
  body.replaceChildren(el("h2", title));
  const get = build(body),
    modal = $("#dialog");
  modal.returnValue = "";
  modal.showModal();
  return new Promise((resolve, reject) =>
    modal.addEventListener(
      "close",
      () => {
        try {
          resolve(modal.returnValue === "ok" ? get() : null);
        } catch (error) {
          reject(error);
        }
      },
      { once: true },
    ),
  );
}
export function modalField(
  parent: HTMLElement,
  label: string,
  value: string | undefined,
  type = "text",
) {
  const wrap = el("label", label),
    node = type === "textarea" ? el("textarea") : el("input");
  if (node instanceof HTMLInputElement) node.type = type;
  node.value = value ?? "";
  node.setAttribute("aria-label", label);
  wrap.append(node);
  parent.append(wrap);
  return node;
}
