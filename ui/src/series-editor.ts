import {
  $,
  el,
  button,
  createInput,
  dialog,
  modalField,
  notice,
  safely,
} from "./dom";
import type { ProjectSummary, BuildJob } from "./models";
export interface SeriesRace {
  title: string;
  project: string | null;
  invitation: string;
}
export interface RaceSeries {
  format: "ghostline-race-series";
  schema_version: 1;
  id: string;
  title: string;
  contact_name: string;
  intro_message: string;
  completion_message: string;
  reward_eddies: number;
  quest_core_slot?: number;
  races: SeriesRace[];
}
interface Options {
  request: (path: string, value?: unknown) => Promise<unknown>;
  projects: () => ProjectSummary[];
  openProject: (id: string) => Promise<void>;
}
export class SeriesEditor {
  private series: RaceSeries | null = null;
  private summaries: ProjectSummary[] = [];
  private timer: ReturnType<typeof setTimeout> | undefined;
  private queue = Promise.resolve();
  private jobTimer: ReturnType<typeof setTimeout> | undefined;
  constructor(private readonly options: Options) {}
  async init() {
    this.summaries = (await this.options.request(
      "series-list",
    )) as ProjectSummary[];
    if (this.summaries[0])
      this.series = (await this.options.request(
        `series/${this.summaries[0].id}`,
      )) as RaceSeries;
    this.render();
  }
  private schedule() {
    $("#series-status").textContent = "Saving…";
    $("#save-status").textContent = "Saving…";
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void safely(() => this.flush())(), 600);
  }
  async flush() {
    clearTimeout(this.timer);
    if (!this.series) return;
    const snapshot = structuredClone(this.series);
    this.queue = this.queue
      .catch(() => {})
      .then(async () => {
        await this.options.request("series-save", snapshot);
        $("#series-status").textContent = "Saved";
        if (!$("#series-view").hidden) $("#save-status").textContent = "Saved";
      });
    await this.queue;
  }
  private async choose(id: string) {
    await this.flush();
    this.series = (await this.options.request(`series/${id}`)) as RaceSeries;
    this.render();
  }
  private async create() {
    const name = await dialog("New championship", (parent) => {
      const title = modalField(
        parent,
        "Championship title",
        "Night City Championship",
      );
      return () => title.value.trim();
    });
    if (!name) return;
    await this.flush();
    this.series = {
      format: "ghostline-race-series",
      schema_version: 1,
      id: "series_" + crypto.randomUUID().replaceAll("-", "").slice(0, 12),
      title: name,
      contact_name: "Race Organiser",
      intro_message:
        "Nine races. Win each one to unlock the next. Win all nine for a €$100,000 championship bonus. Your first start is marked.",
      completion_message:
        "Nine wins. Championship complete. Your €$100,000 bonus has been transferred. Every race stays open for replays.",
      reward_eddies: 100000,
      races: Array.from({ length: 9 }, (_, i) => ({
        title: `Race ${i + 1}`,
        project: null,
        invitation: "",
      })),
    };
    this.summaries.push({ id: this.series.id, title: name });
    this.render();
    await this.flush();
  }
  render() {
    const parent = $("#series-editor");
    parent.replaceChildren();
    const header = el("div", undefined, "section-title");
    header.append(el("h1", "Championship"));
    button(header, "New championship", () => this.create());
    parent.append(header);
    if (this.summaries.length) {
      const select = el("select");
      select.setAttribute("aria-label", "Championship");
      for (const entry of this.summaries)
        select.append(new Option(entry.title, entry.id));
      select.value = this.series?.id ?? "";
      select.onchange = safely(() => this.choose(select.value));
      parent.append(select);
    }
    if (!this.series) {
      parent.append(
        el(
          "p",
          "Create a championship to connect your races with phone invitations, ordered unlocks and a final bonus.",
        ),
      );
      return;
    }
    const series = this.series;
    parent.append(
      el(
        "p",
        `${series.races.filter((r) => r.project).length} of ${series.races.length} courses assigned · €$${series.reward_eddies.toLocaleString()} one-time championship bonus`,
        "hint",
      ),
    );
    const input = createInput(
      () => {},
      () => this.schedule(),
    );
    const basics = el("section", undefined, "panel");
    basics.append(el("h2", "Series setup"));
    input(basics, "Championship title", series.title, (v) => {
      series.title = v;
      const summary = this.summaries.find((s) => s.id === series.id);
      if (summary) summary.title = v;
    });
    input(
      basics,
      "Organiser name",
      series.contact_name,
      (v) => (series.contact_name = v),
    );
    input(
      basics,
      "Intro phone message",
      series.intro_message,
      (v) => (series.intro_message = v),
      { type: "textarea" },
    );
    input(
      basics,
      "Completion reward (€$)",
      series.reward_eddies,
      (v) => (series.reward_eddies = v),
      { type: "number" },
    );
    input(
      basics,
      "Completion phone message",
      series.completion_message,
      (v) => (series.completion_message = v),
      { type: "textarea" },
    );
    parent.append(basics);
    const startupAdvanced = el("details", undefined, "panel");
    startupAdvanced.append(el("summary", "Advanced: quest startup"));
    const startup = el("label", "Quest Core slot"), slot = el("input");
    slot.type = "number";
    slot.min = "1";
    slot.max = "64";
    slot.step = "1";
    slot.placeholder = "Standalone bootstrap";
    slot.value = series.quest_core_slot?.toString() ?? "";
    slot.onchange = () => {
      if (!slot.reportValidity()) return;
      if (slot.value === "") delete series.quest_core_slot;
      else series.quest_core_slot = Number(slot.value);
      this.schedule();
    };
    startup.append(slot);
    startupAdvanced.append(startup, el("p", "Use the slot allocated to this quest pack in Ghostline Quest Core. Leave blank to package its own bootstrap.", "hint"));
    parent.append(startupAdvanced);
    parent.append(el("h2", "Race order"));
    parent.append(
      el(
        "p",
        "The intro unlocks race 1. A first-place finish unlocks the next race. Losses can be retried; won races reopen after the championship. Each unique win counts once.",
      ),
    );
    const assigned = series.races.filter((r) => r.project).length;
    parent.append(
      el(
        "p",
        `${assigned} of ${series.races.length} races assigned · final bonus paid once after all wins`,
        "hint",
      ),
    );
    const list = el("div", undefined, "series-races");
    parent.append(list);
    series.races.forEach((race, index) => {
      const card = el("section", undefined, "panel series-race");
      card.append(el("h3", `${index + 1}. ${race.title}`));
      input(card, "Display title", race.title, (v) => (race.title = v));
      const wrap = el("label", "Race project"),
        select = el("select");
      select.setAttribute("aria-label", `Race ${index + 1} project`);
      select.append(new Option("Choose a saved race", ""));
      for (const project of this.options.projects()) {
        const option = new Option(project.title, project.id);
        option.disabled = series.races.some(
          (r, i) => i !== index && r.project === project.id,
        );
        select.append(option);
      }
      if (
        race.project &&
        !this.options.projects().some((p) => p.id === race.project)
      )
        select.append(
          new Option(`Missing project: ${race.project}`, race.project),
        );
      select.value = race.project ?? "";
      select.onchange = safely(async () => {
        race.project = select.value || null;
        await this.flush();
        this.render();
      });
      wrap.append(select);
      card.append(wrap);
      if (race.project)
        button(card, "Edit race", async () => {
          await this.flush();
          await this.options.openProject(race.project!);
        });
      else
        card.append(
          el("p", "Assign a course before building the championship.", "hint"),
        );
      const advanced = el("details");
      advanced.append(el("summary", "Invitation & order"));
      input(
        advanced,
        "Race invitation (optional)",
        race.invitation,
        (v) => (race.invitation = v),
        { type: "textarea" },
      );
      button(advanced, "Move earlier", async () => {
        if (!index) return;
        [series.races[index - 1], series.races[index]] = [
          series.races[index]!,
          series.races[index - 1]!,
        ];
        await this.flush();
        this.render();
      });
      button(advanced, "Move later", async () => {
        if (index === series.races.length - 1) return;
        [series.races[index + 1], series.races[index]] = [
          series.races[index]!,
          series.races[index + 1]!,
        ];
        await this.flush();
        this.render();
      });
      button(
        advanced,
        "Remove race",
        async () => {
          if (series.races.length === 1)
            throw Error("A championship needs at least one race.");
          series.races.splice(index, 1);
          await this.flush();
          this.render();
        },
        "quiet danger",
      );
      card.append(advanced);
      list.append(card);
    });
    button(parent, "Add race", async () => {
      if (series.races.length >= 32)
        throw Error("A championship supports up to 32 races.");
      series.races.push({
        title: `Race ${series.races.length + 1}`,
        project: null,
        invitation: "",
      });
      await this.flush();
      this.render();
    });
    const status = el("p", "Saved");
    status.id = "series-status";
    status.setAttribute("role", "status");
    parent.append(status);
    const advanced = el("details");
    advanced.append(el("summary", "Advanced series data"));
    button(advanced, "Export championship JSON", async () => {
      await this.flush();
      const a = el("a");
      const url = URL.createObjectURL(
        new Blob([JSON.stringify(series, null, 2)], {
          type: "application/json",
        }),
      );
      a.href = url;
      a.download = series.id + ".series.json";
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 5000);
    });
    parent.append(advanced);
  }
  async check() {
    if (!this.series) throw Error("Create a championship first.");
    await this.flush();
    const result = (await this.options.request(
      "series-check",
      this.series,
    )) as { races: number };
    notice(
      `Championship ready · ${result.races} races · one-time €$${this.series.reward_eddies.toLocaleString()} reward`,
    );
  }
  async build(native: boolean) {
    if (!this.series) throw Error("Create a championship first.");
    await this.flush();
    const { job } = (await this.options.request("series-build", {
      series: this.series,
      native,
    })) as { job: string };
    $("#build").setAttribute("disabled", "");
    notice("Generating championship resources…");
    clearTimeout(this.jobTimer);
    const poll = async () => {
      try {
        const result = (await this.options.request(`job/${job}`)) as BuildJob;
        if (result.status === "running") {
          this.jobTimer = setTimeout(() => void poll(), 1000);
          return;
        }
        $("#build").removeAttribute("disabled");
        if (result.status === "failed") {
          notice(result.error, true);
          return;
        }
        notice(
          `Championship ${native ? "native build" : "resource documents"} complete · ${result.resources} resources`,
        );
        if (native) {
          const link = el("a", "Download championship ZIP");
          link.href = `/api/job/${job}/download`;
          $("#notice").append(document.createTextNode(" · "), link);
        }
      } catch (error) {
        $("#build").removeAttribute("disabled");
        notice(String(error), true);
      }
    };
    await poll();
  }
}
