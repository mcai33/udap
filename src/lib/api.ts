import { Channel, invoke } from "@tauri-apps/api/core";

export interface ProbeInfo {
  id: string;
  name: string;
  vendorId: number;
  productId: number;
  serialNumber: string | null;
  interface: number | null;
  isHidInterface: boolean;
}

export interface TargetInfo {
  name: string;
  family: string;
  architecture: string;
  aliases: string[];
}

export interface DetectRequest {
  probeId: string;
  protocol: "swd" | "jtag";
  speedKhz: number;
  connectUnderReset: boolean;
}

export interface DetectResult {
  target: TargetInfo;
  actualSpeedKhz: number;
}

export interface FlashRequest extends DetectRequest {
  targetName: string;
  firmwarePath: string;
  baseAddress: number | null;
  verify: boolean;
  chipErase: boolean;
  resetAfter: boolean;
}

export interface FlashEvent {
  stage: "connecting" | "erasing" | "filling" | "programming" | "verifying" | "resetting" | "completed" | "message";
  completed: number;
  total: number | null;
  message: string | null;
}

export interface FlashResult {
  targetName: string;
  elapsedMs: number;
}

export function listProbes(): Promise<ProbeInfo[]> {
  return invoke("list_probes");
}

export function listTargets(): Promise<TargetInfo[]> {
  return invoke("list_targets");
}

export function detectTarget(request: DetectRequest): Promise<DetectResult> {
  return invoke("detect_target", { request });
}

export function flashFirmware(request: FlashRequest, onEvent: (event: FlashEvent) => void): Promise<FlashResult> {
  const channel = new Channel<FlashEvent>();
  channel.onmessage = onEvent;
  return invoke("flash_firmware", { request, onEvent: channel });
}

