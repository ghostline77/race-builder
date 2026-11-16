import { Field } from "./Field";
import { ask } from "./Modal";
import { changed, view, notice, safely } from "./state";
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
  private status = "Saved";
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
    changed();
  }
  private schedule() {
    this.status = "Saving…";
    changed();
    view.status = "Saving…";
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
        this.status = "Saved";
        changed();
        if (view.tab === "series") view.status = "Saved";
      });
    await this.queue;
  }
  private async choose(id: string) {
    await this.flush();
    this.series = (await this.options.request(`series/${id}`)) as RaceSeries;
    changed();
  }
  private async create() {
    const values = await ask("New championship", [
      {
        name: "Championship title",
        value: "Night City Championship",
        required: true,
      },
    ]);
    const name = values?.["Championship title"]?.trim();
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
    changed();
    await this.flush();
  }
  render() {
    const series = this.series;
    const edit = (fn: () => void) => {
      fn();
      this.schedule();
      changed();
    };
    const move = (index: number, offset: number) => {
      if (!series) return;
      const target = index + offset;
      if (target < 0 || target >= series.races.length) return;
      edit(() => {
        [series.races[index], series.races[target]] = [
          series.races[target]!,
          series.races[index]!,
        ];
      });
    };
    return (
      <section>
        <h1>Championship</h1>
        <button onClick={safely(() => this.create())}>New championship</button>
        {this.summaries.length > 0 && (
          <select
            aria-label="Championship"
            value={series?.id ?? ""}
            onChange={(e) => void safely(() => this.choose(e.target.value))()}
          >
            {this.summaries.map((s) => (
              <option key={s.id} value={s.id}>
                {s.title}
              </option>
            ))}
          </select>
        )}
        {series && (
          <>
            <section className="panel">
              <h2>Series setup</h2>
              <Field
                label="Championship title"
                value={series.title}
                change={(v) =>
                  edit(() => {
                    series.title = v;
                    const row = this.summaries.find((r) => r.id === series.id);
                    if (row) row.title = v;
                  })
                }
              />
              <Field
                label="Organiser name"
                value={series.contact_name}
                change={(v) =>
                  edit(() => {
                    series.contact_name = v;
                  })
                }
              />
              <Field
                label="Intro phone message"
                type="textarea"
                value={series.intro_message}
                change={(v) =>
                  edit(() => {
                    series.intro_message = v;
                  })
                }
              />
              <Field
                label="Completion phone message"
                type="textarea"
                value={series.completion_message}
                change={(v) =>
                  edit(() => {
                    series.completion_message = v;
                  })
                }
              />
              <Field
                label="Completion reward (€$)"
                type="number"
                value={series.reward_eddies}
                change={(v) =>
                  edit(() => {
                    series.reward_eddies = v;
                  })
                }
              />
              <label>
                Quest Core slot
                <input
                  min={1}
                  max={64}
                  step={1}
                  placeholder="Standalone bootstrap"
                  type="number"
                  value={series.quest_core_slot ?? ""}
                  onChange={(e) => {
                    if (!e.target.validity.valid) return;
                    const value = e.target.value;
                    edit(() => {
                      if (value) series.quest_core_slot = Number(value);
                      else delete series.quest_core_slot;
                    });
                  }}
                />
              </label>
            </section>
            <h2>Race order</h2>
            <p>
              {series.races.filter((r) => r.project).length} of{" "}
              {series.races.length} courses assigned
            </p>
            {series.races.map((race, i) => (
              <section className="panel" key={i}>
                <h3>
                  {i + 1}. {race.title}
                </h3>
                <Field
                  label="Display title"
                  value={race.title}
                  change={(v) =>
                    edit(() => {
                      race.title = v;
                    })
                  }
                />
                <label>
                  Race project
                  <select
                    aria-label={`Race ${i + 1} project`}
                    value={race.project ?? ""}
                    onChange={(e) =>
                      edit(() => {
                        race.project = e.target.value || null;
                      })
                    }
                  >
                    <option value="">Choose a saved race</option>
                    {race.project &&
                      !this.options
                        .projects()
                        .some((p) => p.id === race.project) && (
                        <option value={race.project}>
                          Missing project: {race.project}
                        </option>
                      )}
                    {this.options.projects().map((p) => (
                      <option
                        key={p.id}
                        value={p.id}
                        disabled={series.races.some(
                          (r, n) => n !== i && r.project === p.id,
                        )}
                      >
                        {p.title}
                      </option>
                    ))}
                  </select>
                </label>
                {race.project && (
                  <button
                    onClick={safely(async () => {
                      await this.flush();
                      await this.options.openProject(race.project!);
                    })}
                  >
                    Edit race
                  </button>
                )}
                <details>
                  <summary>Invitation &amp; order</summary>
                  <Field
                    label="Race invitation (optional)"
                    type="textarea"
                    value={race.invitation}
                    change={(v) =>
                      edit(() => {
                        race.invitation = v;
                      })
                    }
                  />
                  <button disabled={!i} onClick={() => move(i, -1)}>
                    Move earlier
                  </button>
                  <button
                    disabled={i === series.races.length - 1}
                    onClick={() => move(i, 1)}
                  >
                    Move later
                  </button>
                  <button
                    disabled={series.races.length === 1}
                    onClick={() =>
                      edit(() => {
                        series.races.splice(i, 1);
                      })
                    }
                  >
                    Remove race
                  </button>
                </details>
              </section>
            ))}
            <button
              disabled={series.races.length >= 32}
              onClick={() =>
                edit(() => {
                  series.races.push({
                    title: `Race ${series.races.length + 1}`,
                    project: null,
                    invitation: "",
                  });
                })
              }
            >
              Add race
            </button>
            <p role="status">{this.status}</p>
            <button
              onClick={safely(async () => {
                await this.flush();
                const url = URL.createObjectURL(
                  new Blob([JSON.stringify(series, null, 2)], {
                    type: "application/json",
                  }),
                );
                const a = document.createElement("a");
                a.href = url;
                a.download = series.id + ".series.json";
                a.click();
                setTimeout(() => URL.revokeObjectURL(url), 5000);
              })}
            >
              Export championship JSON
            </button>
          </>
        )}
      </section>
    );
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
    view.busy = true;
    changed();
    notice("Generating championship resources…");
    clearTimeout(this.jobTimer);
    const poll = async () => {
      try {
        const result = (await this.options.request(`job/${job}`)) as BuildJob;
        if (result.status === "running") {
          this.jobTimer = setTimeout(() => void poll(), 1000);
          return;
        }
        view.busy = false;
        changed();
        if (result.status === "failed") {
          notice(result.error, true);
          return;
        }
        notice(
          `Championship ${native ? "native build" : "resource documents"} complete · ${result.resources} resources`,
        );
        if (native) {
          view.download = `/api/job/${job}/download`;
          changed();
        }
      } catch (error) {
        view.busy = false;
        changed();
        notice(String(error), true);
      }
    };
    await poll();
  }
}
