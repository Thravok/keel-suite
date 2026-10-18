export type LayoutKind = "mini" | "original" | "xl";
export type MediaCommand = "play_pause" | "next" | "previous" | "volume_up" | "volume_down" | "mute";
export type ActionSpec =
  | { type: "empty" }
  | { type: "hotkey"; cmd: boolean; ctrl: boolean; alt: boolean; shift: boolean; key: string }
  | { type: "open_url"; url: string }
  | { type: "open_app"; path: string }
  | { type: "run_command"; program: string; args: string[] }
  | { type: "media"; command: MediaCommand }
  | { type: "text"; text: string }
  | { type: "brightness"; percent: number }
  | { type: "go_page"; page_id: string }
  | { type: "go_back" };
export interface KeyButton { id: string; title: string; color: string; image?: string | null; action: ActionSpec; }
export interface Page { id: string; name: string; keys: KeyButton[]; }
export interface Profile { id: string; name: string; layout: LayoutKind; pages: Page[]; current_page_id: string; page_stack: string[]; }
export interface AppSettings { active_profile_id: string | null; brightness: number; start_hidden: boolean; }
export interface DeviceInfo { kind: string; serial: string; rows: number; cols: number; key_count: number; encoder_count: number; }
export interface AppStateSnapshot { profiles: Profile[]; settings: AppSettings; devices: DeviceInfo[]; }
export function layoutDims(layout: LayoutKind): { rows: number; cols: number } {
  switch (layout) { case "mini": return { rows: 2, cols: 3 }; case "xl": return { rows: 4, cols: 8 }; default: return { rows: 3, cols: 5 }; }
}
export function emptyAction(): ActionSpec { return { type: "empty" }; }
