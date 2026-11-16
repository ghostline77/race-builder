import type { Bridge, Camera, Point, Road } from "./road-levels";

export type Json =
  | null
  | boolean
  | number
  | string
  | Json[]
  | { [key: string]: Json | undefined };
export interface MarkOptions {
  generated_road_sample?: boolean;
  route_anchor?: boolean;
  width_m?: number;
  depth_m?: number;
  height_m?: number;
  speed_mps?: number;
  left_m?: number;
  right_m?: number;
  entity_template?: string;
  [key: string]: Json | undefined;
}
export interface RouteMark {
  id: string;
  role: string;
  order: number;
  name: string;
  notes: string;
  position: Point;
  yaw: number;
  options: MarkOptions;
  evidence: { source: string; [key: string]: Json | undefined };
}
export interface Route {
  format: "ghostline-race-route";
  schema_version: 1;
  id: string;
  title: string;
  description: string;
  prefix: string;
  looped: boolean;
  width_default: number;
  speed_mps: number;
  marks: RouteMark[];
}
export interface Entrant {
  id: string;
  name: string;
  grid: number;
  vehicle: string;
  vehicle_appearance?: string;
  driver: string;
  driver_appearance?: string;
  passenger?: string;
  passenger_appearance?: string;
}
export interface RaceConfig {
  schema_version: 1;
  id: string;
  title: string;
  description: string;
  route: string;
  entrants: Entrant[];
  checkpoint_spacing_m?: number;
  checkpoint_placement?: "automatic" | "manual";
  checkpoint_width_m?: number;
  guidance_alignment?: "route" | "road_center";
  countdown_seconds?: number;
  timeout_seconds?: number;
  checkpoint_mode?: string;
  player_vehicle?: { record: string; appearance: string };
}
export interface Project {
  route: Route;
  config: RaceConfig;
}
export interface ProjectSummary {
  id: string;
  title: string;
}
export interface ToolSettings {
  schema: string;
  game: string;
}
export interface Bootstrap {
  token: string;
  projects: ProjectSummary[];
  settings: ToolSettings;
  roles: string[];
}
export interface NetworkRoad extends Omit<Road, "points" | "bounds"> {
  points: (Point | [number, number, number])[];
  drivable?: boolean;
}
export interface RoadNetwork {
  map_display: { version: 1; bridges: Bridge[] };
  format: string;
  schema_version: number;
  topology?: string;
  roads: NetworkRoad[];
  connections?: {
    from: string;
    to: string;
    from_position?: Point;
    to_position?: Point;
  }[];
  failures?: Json[];
}
export interface Vehicle {
  record?: string;
  record_id?: string;
  id?: string;
  name?: string;
}
export interface SavedView {
  project: string;
  camera: Camera;
}
export type BuildJob =
  | { status: "running" }
  | { status: "complete"; native: boolean; resources: number; output: string }
  | { status: "failed"; error: string };
export interface ApiReplies {
  bootstrap: Bootstrap;
  roads: RoadNetwork;
  vehicles: Vehicle[];
  save: { projects: ProjectSummary[] };
  snap: { position: Point; road: string; distance: number };
  path: { points: Point[] };
  guidance: { points: Point[] };
  import: Route;
  settings: ToolSettings;
  check: { marks: number };
  build: { job: string };
}
