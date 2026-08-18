<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    detectTarget,
    flashFirmware,
    listProbes,
    listTargets,
    type FlashEvent,
    type ProbeInfo,
    type TargetInfo
  } from "./lib/api";

  let probes: ProbeInfo[] = [];
  let targets: TargetInfo[] = [];
  let selectedProbeId = "";
  let selectedTargetName = "";
  let detectedTarget: TargetInfo | null = null;
  let targetQuery = "";
  let firmwarePath = "";
  let protocol: "swd" | "jtag" = "swd";
  let speedKhz = 4000;
  let connectUnderReset = false;
  let baseAddress = "0x08000000";
  let verify = true;
  let chipErase = false;
  let resetAfter = true;
  let loadingProbes = false;
  let detecting = false;
  let flashing = false;
  let status = "准备就绪";
  let errorMessage = "";
  let progress = 0;

  function formatError(error: unknown): string {
    if (typeof error === "string") return error;
    if (error && typeof error === "object") {
      const value = error as { message?: string; detail?: string };
      if (value.message) return value.detail ? `${value.message}\n${value.detail}` : value.message;
    }
    return "发生未知错误";
  }

  $: selectedProbe = probes.find((probe) => probe.id === selectedProbeId) ?? null;
  $: selectedTarget = targets.find((target) => target.name === selectedTargetName) ?? null;
  $: filteredTargets = targetQuery.trim()
    ? targets.filter((target) => {
        const query = targetQuery.trim().toLowerCase();
        return `${target.name} ${target.family} ${target.aliases.join(" ")}`.toLowerCase().includes(query);
      })
    : targets;
  $: firmwareIsBin = firmwarePath.toLowerCase().endsWith(".bin");
  $: canFlash = Boolean(selectedProbe && selectedTarget && firmwarePath) && !flashing;

  async function refreshProbes() {
    loadingProbes = true;
    errorMessage = "";
    try {
      probes = await listProbes();
      if (probes.length === 1) selectedProbeId = probes[0].id;
      status = probes.length ? `发现 ${probes.length} 个调试器` : "未发现调试器";
    } catch (error) {
      errorMessage = formatError(error);
    } finally {
      loadingProbes = false;
    }
  }

  async function loadTargets() {
    try {
      targets = await listTargets();
    } catch (error) {
      errorMessage = `无法加载目标列表：${formatError(error)}`;
    }
  }

  async function runDetection() {
    if (!selectedProbeId) return;
    detecting = true;
    errorMessage = "";
    status = "正在读取目标芯片识别信息…";
    try {
      const result = await detectTarget({ probeId: selectedProbeId, protocol, speedKhz, connectUnderReset });
      detectedTarget = result.target;
      selectedTargetName = result.target.name;
      speedKhz = result.actualSpeedKhz;
      status = `已识别 ${result.target.name}`;
    } catch (error) {
      detectedTarget = null;
      errorMessage = formatError(error);
      status = "自动探测失败，请搜索并选择目标型号";
    } finally {
      detecting = false;
    }
  }

  async function chooseFirmware() {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "固件", extensions: ["elf", "axf", "hex", "ihex", "bin", "uf2"] }]
    });
    if (typeof path === "string") firmwarePath = path;
  }

  function handleFlashEvent(event: FlashEvent) {
    const labels = {
      connecting: "正在连接目标",
      erasing: "正在擦除 Flash",
      filling: "正在保留未写区域",
      programming: "正在写入固件",
      verifying: "正在校验固件",
      resetting: "正在复位目标",
      completed: "烧录完成",
      message: event.message ?? "烧录器消息"
    };
    status = labels[event.stage];
    progress = event.total && event.total > 0 ? Math.min(100, Math.round((event.completed / event.total) * 100)) : progress;
    if (event.stage === "completed") progress = 100;
  }

  async function runFlash() {
    if (!canFlash || !selectedTarget) return;
    const parsedAddress = firmwareIsBin ? Number.parseInt(baseAddress, 0) : null;
    if (firmwareIsBin && !Number.isFinite(parsedAddress)) {
      errorMessage = "BIN 文件需要有效的烧录基地址，例如 0x08000000";
      return;
    }

    flashing = true;
    progress = 0;
    errorMessage = "";
    try {
      const result = await flashFirmware(
        {
          probeId: selectedProbeId,
          targetName: selectedTarget.name,
          firmwarePath,
          protocol,
          speedKhz,
          connectUnderReset,
          baseAddress: parsedAddress,
          verify,
          chipErase,
          resetAfter
        },
        handleFlashEvent
      );
      status = `烧录完成 · ${(result.elapsedMs / 1000).toFixed(1)} 秒`;
      progress = 100;
    } catch (error) {
      errorMessage = formatError(error);
      status = "烧录失败";
    } finally {
      flashing = false;
    }
  }

  refreshProbes();
  loadTargets();
</script>

<svelte:head><title>uDAP Programmer</title></svelte:head>

<main>
  <header class="hero">
    <div>
      <p class="eyebrow">UNIVERSAL DAP PROGRAMMER</p>
      <h1>连接。识别。烧录。</h1>
      <p class="subtitle">通过 CMSIS-DAP 为 ARM 与 RISC-V 目标提供可靠的在线烧录。</p>
    </div>
    <div class="online"><span></span> 在线模式</div>
  </header>

  <section class="workspace">
    <div class="flow">
      <article class="card">
        <div class="card-title"><span class="step">01</span><div><h2>调试器</h2><p>选择已连接的 DAPLink</p></div></div>
        <div class="field-row">
          <select bind:value={selectedProbeId} disabled={loadingProbes || flashing}>
            <option value="">{loadingProbes ? "正在扫描…" : "请选择调试器"}</option>
            {#each probes as probe}
              <option value={probe.id}>{probe.name} · {probe.vendorId.toString(16).padStart(4, "0")}:{probe.productId.toString(16).padStart(4, "0")}</option>
            {/each}
          </select>
          <button class="secondary icon-button" onclick={refreshProbes} disabled={loadingProbes || flashing} title="重新扫描">↻</button>
        </div>
        {#if selectedProbe}
          <div class="detail-line"><span>序列号</span><strong>{selectedProbe.serialNumber ?? "设备未提供"}</strong></div>
        {/if}
      </article>

      <article class="card">
        <div class="card-title"><span class="step">02</span><div><h2>目标 MCU</h2><p>优先自动探测，也可以手动选择</p></div></div>
        <button class="detect" onclick={runDetection} disabled={!selectedProbeId || detecting || flashing}>
          {detecting ? "正在探测…" : "自动探测目标"}
        </button>
        {#if detectedTarget}
          <div class="detected"><span>✓</span><div><small>已自动识别</small><strong>{detectedTarget.name}</strong></div></div>
        {/if}
        <label class="input-label" for="target-search">目标型号</label>
        <input id="target-search" type="search" bind:value={targetQuery} placeholder={`搜索 ${targets.length} 个受支持目标`} disabled={flashing} />
        <select class="target-select" bind:value={selectedTargetName} size="4" disabled={flashing}>
          {#each filteredTargets.slice(0, 200) as target}
            <option value={target.name}>{target.name} — {target.family}</option>
          {/each}
        </select>
        {#if filteredTargets.length > 200}<p class="hint">结果较多，仅显示前 200 项，请继续输入型号。</p>{/if}
      </article>

      <article class="card">
        <div class="card-title"><span class="step">03</span><div><h2>固件</h2><p>支持 ELF、HEX、BIN 与 UF2</p></div></div>
        <button class="file-picker" onclick={chooseFirmware} disabled={flashing}>
          <span>{firmwarePath ? firmwarePath.split(/[\\/]/).pop() : "选择固件文件"}</span><b>浏览</b>
        </button>
        {#if firmwarePath}<p class="path" title={firmwarePath}>{firmwarePath}</p>{/if}
        {#if firmwareIsBin}
          <label class="input-label" for="base-address">BIN 烧录基地址</label>
          <input id="base-address" bind:value={baseAddress} placeholder="0x08000000" disabled={flashing} />
        {/if}
      </article>
    </div>

    <aside class="panel">
      <div class="panel-heading"><p class="eyebrow">PROGRAM SETTINGS</p><h2>烧录设置</h2></div>
      <label>接口协议<select bind:value={protocol} disabled={flashing}><option value="swd">SWD</option><option value="jtag">JTAG</option></select></label>
      <label>调试时钟<div class="suffix"><input type="number" min="50" step="50" bind:value={speedKhz} disabled={flashing} /><span>kHz</span></div></label>
      <label class="check"><input type="checkbox" bind:checked={connectUnderReset} disabled={flashing} /><span><strong>复位下连接</strong><small>普通连接失败或芯片锁死时使用</small></span></label>
      <label class="check"><input type="checkbox" bind:checked={verify} disabled={flashing} /><span><strong>烧录后校验</strong><small>读取 Flash 并验证写入内容</small></span></label>
      <label class="check"><input type="checkbox" bind:checked={chipErase} disabled={flashing} /><span><strong>全片擦除</strong><small>会清除芯片中的全部可擦除内容</small></span></label>
      <label class="check"><input type="checkbox" bind:checked={resetAfter} disabled={flashing} /><span><strong>完成后复位运行</strong></span></label>

      <div class="summary">
        <div><span>调试器</span><strong>{selectedProbe?.name ?? "未选择"}</strong></div>
        <div><span>目标</span><strong>{selectedTarget?.name ?? "未选择"}</strong></div>
      </div>

      <button class="primary" onclick={runFlash} disabled={!canFlash}>{flashing ? "烧录进行中…" : "开始烧录"}<span>→</span></button>
      <div class="progress-track"><div class="progress-value" style={`width: ${progress}%`}></div></div>
      <div class="status"><span class:busy={flashing}></span><p>{status}</p><b>{progress}%</b></div>
      {#if errorMessage}<div class="error">{errorMessage}</div>{/if}
    </aside>
  </section>
</main>
