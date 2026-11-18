import { Mesh, Program, Renderer, Texture, Triangle } from "ogl";

type Rgb = [number, number, number];
type Palette = [Rgb, Rgb, Rgb, Rgb];
type Point = { x: number; y: number };
type Rect = { left: number; top: number; width: number; height: number };

type Glint =
  | { kind: "rect"; x: number; y: number; width: number; height: number; radius: number }
  | { kind: "line"; x1: number; y1: number; x2: number; y2: number };

type RingCanvas = {
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  width: number;
  height: number;
  dpr: number;
  dark: boolean;
};

type Lens = {
  canvas: HTMLCanvasElement;
  renderer: Renderer;
  program: Program;
  texture: Texture;
  mesh: Mesh;
};

const AURORA: Record<"light" | "dark", Palette> = {
  light: [
    [110, 150, 255],
    [180, 130, 255],
    [255, 130, 190],
    [255, 180, 120],
  ],
  dark: [
    [80, 140, 255],
    [160, 110, 255],
    [255, 110, 180],
    [255, 190, 140],
  ],
};

const RING_WIDTH = 110;
const START_RADIUS = 650;
const GROWTH = 800;
const ARC_DEPTH = 650;
const EDGE_FADE = 56;
const DURATION = 1000;
const MAX_DPR = 2;
const MAX_GLINTS = 900;
const SKIP_TAGS = new Set([
  "SPAN",
  "CODE",
  "EM",
  "STRONG",
  "B",
  "I",
  "SMALL",
  "P",
  "H1",
  "H2",
  "H3",
  "H4",
  "H5",
  "H6",
  "SVG",
  "PATH",
  "G",
  "CIRCLE",
  "RECT",
  "LINE",
  "POLYLINE",
  "POLYGON",
  "TSPAN",
  "DEFS",
  "USE",
  "TEXT",
]);

let active = false;
let stopActive: (() => void) | null = null;

export function installHaloTrigger(): () => void {
  const onPointer = (event: PointerEvent) => {
    if (event.button !== 0) return;
    const target = event.target;
    if (target instanceof Element && target.closest(".composer-input")) startHalo();
  };
  const onFocus = (event: FocusEvent) => {
    const target = event.target;
    if (target instanceof Element && target.closest(".composer-input")) startHalo();
  };
  document.addEventListener("pointerdown", onPointer, true);
  document.addEventListener("focusin", onFocus, true);
  return () => {
    document.removeEventListener("pointerdown", onPointer, true);
    document.removeEventListener("focusin", onFocus, true);
    stopActive?.();
  };
}

function startHalo() {
  if (active) return;
  const main = document.querySelector<HTMLElement>(".main");
  const input = document.querySelector<HTMLElement>(".composer-input");
  if (!main || !input) return;
  const tools = document.querySelector<HTMLElement>(".composer-tools");
  const mainRect = main.getBoundingClientRect();
  const inputRect = input.getBoundingClientRect();
  const toolsRect = tools?.getBoundingClientRect();
  const bottom = Math.min(mainRect.bottom, toolsRect?.top ?? mainRect.bottom);
  const area: Rect = {
    left: mainRect.left,
    top: mainRect.top,
    width: mainRect.width,
    height: bottom - mainRect.top,
  };
  if (area.width < 40 || area.height < 40) return;

  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR);
  const origin: Point = {
    x: inputRect.left + inputRect.width / 2 - area.left,
    y: area.height + ARC_DEPTH,
  };
  const originAbs: Point = {
    x: inputRect.left + inputRect.width / 2,
    y: area.top + area.height + ARC_DEPTH,
  };
  const transcript = document.querySelector<HTMLElement>(".thread-scroll-wrap");
  const lensRect = transcript ? clip(transcript.getBoundingClientRect(), area) : null;
  const bendBottom = lensRect ? lensRect.top + lensRect.height - area.top : 0;
  const glints = reduced ? [] : collectGlints(main, area, tools);
  const ring = createRingCanvas(area, dpr);
  document.body.append(ring.canvas);

  active = true;
  const t0 = performance.now();
  let frame = 0;
  let lens: Lens | null = null;
  let disposed = false;
  let dark = isDark();

  const dropLens = () => {
    if (!lens) return;
    disposeLens(lens);
    lens = null;
  };

  const stop = () => {
    if (disposed) return;
    disposed = true;
    cancelAnimationFrame(frame);
    dropLens();
    ring.canvas.remove();
    document.removeEventListener("scroll", dropLens, true);
    document.removeEventListener("pointerdown", onPointer, true);
    active = false;
    stopActive = null;
  };
  stopActive = stop;

  const onPointer = (event: PointerEvent) => {
    if (event.target instanceof Node && transcript?.contains(event.target)) dropLens();
  };
  document.addEventListener("scroll", dropLens, true);
  document.addEventListener("pointerdown", onPointer, true);

  const tick = (now: number) => {
    if (disposed) return;
    const elapsed = Math.max(0, now - t0);
    const t = reduced ? clamp(elapsed / 600, 0, 1) : clamp(elapsed / DURATION, 0, 1);
    const radius = reduced
      ? START_RADIUS
      : START_RADIUS + (GROWTH * (1 - 2 ** (-9 * t))) / (1 - 2 ** -9);
    const strength = reduced
      ? 0.5 * Math.sin(Math.PI * t)
      : Math.min(1, t / 0.03) * (1 - t ** 2.2);
    const nextDark = isDark();
    if (nextDark !== dark) {
      dark = nextDark;
      dropLens();
    }
    drawRings(ring, origin, radius, strength, dark, glints, bendBottom, lens !== null);
    if (lens) drawLens(lens, radius, strength);
    if ((reduced && elapsed >= 600) || (!reduced && elapsed >= DURATION)) {
      stop();
      return;
    }
    frame = requestAnimationFrame(tick);
  };
  frame = requestAnimationFrame(tick);

  if (!reduced && transcript && lensRect) {
    requestAnimationFrame(() => {
      if (disposed) return;
      const scroller = transcript.querySelector<HTMLElement>(".thread-scroll");
      const scrollTop = scroller?.scrollTop ?? 0;
      void captureLens(transcript, lensRect, dpr)
        .then((snapshot) => {
          if (!snapshot || disposed) return;
          const current = transcript.querySelector<HTMLElement>(".thread-scroll");
          if ((current?.scrollTop ?? 0) !== scrollTop) return;
          lens = createLens(snapshot, lensRect, originAbs, dpr);
          if (!lens) return;
          document.body.append(lens.canvas);
        })
        .catch(() => undefined);
    });
  }
}

function createRingCanvas(area: Rect, dpr: number): RingCanvas {
  const canvas = makeCanvas("halo-rings", area, dpr);
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("2d canvas unavailable");
  return {
    canvas,
    ctx,
    width: area.width,
    height: area.height,
    dpr,
    dark: isDark(),
  };
}

function drawRings(
  ring: RingCanvas,
  origin: Point,
  radius: number,
  strength: number,
  dark: boolean,
  glints: Glint[],
  bendBottom: number,
  bend: boolean,
) {
  const { ctx, width, height, dpr } = ring;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  if (strength <= 0.001) return;

  if (ring.dark !== dark) {
    ring.dark = dark;
    ring.canvas.dataset.dark = String(dark);
    ring.canvas.style.mixBlendMode = dark ? "screen" : "normal";
  }

  const palette = AURORA[dark ? "dark" : "light"];
  const r0 = Math.max(0, radius - 5 * RING_WIDTH);
  const r1 = radius + 5 * RING_WIDTH;
  const gradient = ctx.createRadialGradient(origin.x, origin.y, r0, origin.x, origin.y, r1);
  const steps = 96;
  for (let i = 0; i <= steps; i += 1) {
    const x = -5 + (10 * i) / steps;
    const stop = clamp((radius + x * RING_WIDTH - r0) / (r1 - r0), 0, 1);
    const [r, g, b] = ringColorAt(x, palette);
    const alpha = strength * Math.exp(-1.1 * (x / 2) ** 2) * (dark ? 0.45 : 0.26);
    gradient.addColorStop(stop, `rgba(${r | 0}, ${g | 0}, ${b | 0}, ${alpha.toFixed(4)})`);
  }
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, width, height);

  if (!glints.length) return;
  ctx.globalCompositeOperation = "source-over";
  ctx.lineWidth = 1;
  ctx.lineCap = "butt";
  for (const glint of glints) {
    const midpoint =
      glint.kind === "rect"
        ? { x: glint.x + glint.width / 2, y: glint.y + glint.height / 2 }
        : { x: (glint.x1 + glint.x2) / 2, y: (glint.y1 + glint.y2) / 2 };
    const bent = bend && midpoint.y < bendBottom;
    const push = bent ? pushAt(midpoint, origin, radius) : { x: 0, y: 0 };
    const bentMid = { x: midpoint.x + push.x, y: midpoint.y + push.y };
    const front = (distance(bentMid, origin) - radius) / RING_WIDTH;
    const alpha =
      strength *
      Math.exp(-1.4 * (front / 2) ** 2) *
      (dark ? 0.14 : 0.3);
    if (alpha < 0.004) continue;
    const [cr, cg, cb] = mixWhite(ringColorAt(front, palette), 0.2);
    ctx.strokeStyle = `rgba(${cr | 0}, ${cg | 0}, ${cb | 0}, ${alpha.toFixed(4)})`;

    if (glint.kind === "rect") {
      const x = glint.x + push.x + 0.5;
      const y = glint.y + push.y + 0.5;
      const w = Math.max(0, glint.width - 1);
      const h = Math.max(0, glint.height - 1);
      if (w < 1 || h < 1) continue;
      ctx.beginPath();
      ctx.roundRect(x, y, w, h, Math.min(glint.radius, w / 2, h / 2));
      ctx.stroke();
      continue;
    }

    const from = bentPoint(glint.x1, glint.y1, origin, radius, bent);
    const to = bentPoint(glint.x2, glint.y2, origin, radius, bent);
    ctx.beginPath();
    ctx.moveTo(from.x, from.y);
    ctx.lineTo(to.x, to.y);
    ctx.stroke();
  }
}

function bentPoint(x: number, y: number, origin: Point, radius: number, bent: boolean): Point {
  if (!bent) return { x, y };
  const push = pushAt({ x, y }, origin, radius);
  return { x: x + push.x, y: y + push.y };
}

function pushAt(point: Point, origin: Point, radius: number): Point {
  const dx = point.x - origin.x;
  const dy = point.y - origin.y;
  const dist = Math.hypot(dx, dy);
  if (dist < 0.01) return { x: 0, y: 0 };
  const x = (dist - radius) / RING_WIDTH;
  const z = x * 0.75;
  const damp = 1 - smoothstep(0.9, 2.2, Math.abs(x));
  const push = 7 * 2.33 * z * Math.exp(-z * z) * damp;
  return { x: (dx / dist) * push, y: (dy / dist) * push };
}

function ringColorAt(x: number, palette: Palette): Rgb {
  const weights = palette.map((_, index) => Math.exp(-(((x - (index - 1.5)) / 0.8) ** 2)));
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  if (total < 1e-9) {
    const index = clamp(Math.round(x + 1.5), 0, 3);
    return palette[index];
  }
  let r = 0;
  let g = 0;
  let b = 0;
  for (let index = 0; index < 4; index += 1) {
    const weight = weights[index] / total;
    r += palette[index][0] * weight;
    g += palette[index][1] * weight;
    b += palette[index][2] * weight;
  }
  return [r, g, b];
}

function mixWhite(color: Rgb, amount: number): Rgb {
  return [
    color[0] + (255 - color[0]) * amount,
    color[1] + (255 - color[1]) * amount,
    color[2] + (255 - color[2]) * amount,
  ];
}

async function captureLens(
  transcript: HTMLElement,
  rect: Rect,
  dpr: number,
): Promise<HTMLCanvasElement> {
  const main = document.querySelector<HTMLElement>(".main");
  const background = main ? getComputedStyle(main).backgroundColor : "#ffffff";
  const { default: html2canvas } = await import("html2canvas-pro");
  const canvas = await html2canvas(transcript, {
    width: rect.width,
    height: rect.height,
    scale: Math.min(dpr, 1.5),
    backgroundColor: background,
    logging: false,
    useCORS: true,
    imageTimeout: 1000,
    scrollX: 0,
    scrollY: 0,
  });
  return canvas;
}

function createLens(
  snapshot: HTMLCanvasElement,
  rect: Rect,
  originAbs: Point,
  dpr: number,
): Lens | null {
  const canvas = makeCanvas("halo-lens", rect, dpr);
  let renderer: Renderer;
  try {
    renderer = new Renderer({
      canvas,
      width: rect.width,
      height: rect.height,
      dpr,
      alpha: true,
      premultipliedAlpha: true,
      depth: false,
      stencil: false,
      antialias: false,
    });
  } catch {
    canvas.remove();
    return null;
  }

  const gl = renderer.gl;
  gl.clearColor(0, 0, 0, 0);
  const texture = new Texture(gl, {
    image: snapshot,
    flipY: false,
    generateMipmaps: false,
    minFilter: gl.LINEAR,
    magFilter: gl.LINEAR,
  });
  const program = new Program(gl, {
    vertex: VERTEX,
    fragment: FRAGMENT,
    transparent: true,
    depthTest: false,
    depthWrite: false,
    uniforms: {
      uTexture: { value: texture },
      uResolution: { value: [rect.width, rect.height] },
      uOrigin: { value: [originAbs.x - rect.left, originAbs.y - rect.top] },
      uRadius: { value: START_RADIUS },
      uStrength: { value: 0 },
      uOpacity: { value: 1 },
      uWidth: { value: RING_WIDTH },
      uEdge: { value: EDGE_FADE },
    },
  });
  const mesh = new Mesh(gl, { geometry: new Triangle(gl), program });
  renderer.render({ scene: mesh });
  return {
    canvas,
    renderer,
    program,
    texture,
    mesh,
  };
}

function drawLens(lens: Lens, radius: number, strength: number) {
  const uniforms = lens.program.uniforms;
  uniforms.uRadius.value = radius;
  uniforms.uStrength.value = strength;
  uniforms.uOpacity.value = 1;
  lens.renderer.render({ scene: lens.mesh });
}

function disposeLens(lens: Lens) {
  lens.canvas.remove();
  lens.program.remove();
  lens.renderer.gl.deleteTexture(lens.texture.texture);
  lens.renderer.gl.getExtension("WEBGL_lose_context")?.loseContext();
}

function makeCanvas(className: string, rect: Rect, dpr: number): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.className = `halo-canvas ${className}`;
  canvas.dataset.dark = String(isDark());
  canvas.width = Math.max(1, Math.round(rect.width * dpr));
  canvas.height = Math.max(1, Math.round(rect.height * dpr));
  const style = canvas.style;
  style.left = `${rect.left}px`;
  style.top = `${rect.top}px`;
  style.width = `${rect.width}px`;
  style.height = `${rect.height}px`;
  style.mixBlendMode = className === "halo-rings" && isDark() ? "screen" : "normal";
  canvas.setAttribute("aria-hidden", "true");
  return canvas;
}

function collectGlints(main: HTMLElement, area: Rect, tools: HTMLElement | null): Glint[] {
  const glints: Glint[] = [];
  const right = area.left + area.width;
  const bottom = area.top + area.height;
  const nodes = main.querySelectorAll<HTMLElement>("*");
  for (const node of nodes) {
    if (glints.length >= MAX_GLINTS) break;
    if (SKIP_TAGS.has(node.tagName)) continue;
    if (tools?.contains(node)) continue;
    if (node.closest("code, .hljs")) continue;
    const rect = node.getBoundingClientRect();
    if (rect.width < 2 && rect.height < 2) continue;
    if (rect.right < area.left || rect.left > right || rect.bottom < area.top || rect.top > bottom) {
      continue;
    }
    const style = getComputedStyle(node);
    if (style.display === "none" || style.visibility === "hidden" || style.opacity === "0") {
      continue;
    }

    const widths = [
      parseFloat(style.borderTopWidth) || 0,
      parseFloat(style.borderRightWidth) || 0,
      parseFloat(style.borderBottomWidth) || 0,
      parseFloat(style.borderLeftWidth) || 0,
    ];
    const colors = [
      style.borderTopColor,
      style.borderRightColor,
      style.borderBottomColor,
      style.borderLeftColor,
    ];
    const visible = widths.map((width, index) => width > 0.25 && hasColor(colors[index]));
    const x = rect.left - area.left;
    const y = rect.top - area.top;

    if (visible.every(Boolean)) {
      glints.push({
        kind: "rect",
        x,
        y,
        width: rect.width,
        height: rect.height,
        radius: parseFloat(style.borderTopLeftRadius) || 0,
      });
      continue;
    }
    if (visible.some(Boolean)) {
      if (visible[0]) glints.push(line(x, y, x + rect.width, y));
      if (visible[1]) glints.push(line(x + rect.width, y, x + rect.width, y + rect.height));
      if (visible[2]) glints.push(line(x, y + rect.height, x + rect.width, y + rect.height));
      if (visible[3]) glints.push(line(x, y, x, y + rect.height));
      continue;
    }

    if (!hasColor(style.backgroundColor)) continue;
    if (rect.height <= 1.5 && rect.width >= 8) {
      glints.push(line(x, y + rect.height / 2, x + rect.width, y + rect.height / 2));
    } else if (rect.width <= 1.5 && rect.height >= 8) {
      glints.push(line(x + rect.width / 2, y, x + rect.width / 2, y + rect.height));
    }
  }
  return glints;
}

function line(x1: number, y1: number, x2: number, y2: number): Glint {
  return { kind: "line", x1, y1, x2, y2 };
}

function hasColor(value: string): boolean {
  if (!value || value === "transparent") return false;
  const slash = value.match(/\/\s*([\d.]+)\s*\)/);
  if (slash) return Number(slash[1]) > 0.01;
  const rgba = value.match(/^rgba?\(([^)]+)\)$/);
  if (!rgba) return true;
  const parts = rgba[1].split(/[,\s]+/).filter(Boolean);
  return parts.length < 4 || Number(parts[3]) > 0.01;
}

function clip(rect: DOMRect, area: Rect): Rect | null {
  const left = Math.max(rect.left, area.left);
  const top = Math.max(rect.top, area.top);
  const right = Math.min(rect.right, area.left + area.width);
  const bottom = Math.min(rect.bottom, area.top + area.height);
  if (right - left < 20 || bottom - top < 20) return null;
  return { left, top, width: right - left, height: bottom - top };
}

function isDark(): boolean {
  return document.documentElement.dataset.mantineColorScheme === "dark";
}

function distance(a: Point, b: Point): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

function smoothstep(edge0: number, edge1: number, value: number): number {
  const t = clamp((value - edge0) / (edge1 - edge0), 0, 1);
  return t * t * (3 - 2 * t);
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

const VERTEX = /* glsl */ `
attribute vec2 uv;
attribute vec2 position;
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = vec4(position, 0.0, 1.0);
}
`;

const FRAGMENT = /* glsl */ `
precision highp float;
varying vec2 vUv;
uniform sampler2D uTexture;
uniform vec2 uResolution;
uniform vec2 uOrigin;
uniform float uRadius;
uniform float uStrength;
uniform float uOpacity;
uniform float uWidth;
uniform float uEdge;

void main() {
  vec2 p = vec2(vUv.x, 1.0 - vUv.y) * uResolution;
  vec2 delta = p - uOrigin;
  float dist = length(delta);
  vec2 dir = dist > 0.001 ? delta / dist : vec2(0.0);
  float x = (dist - uRadius) / uWidth;
  float z = x * 0.75;
  float band = 1.0 - smoothstep(1.4, 2.2, abs(x));
  float damp = 1.0 - smoothstep(0.9, 2.2, abs(x));
  float push = 7.0 * 2.33 * z * exp(-z * z) * damp;
  vec2 offset = dir * push;
  vec2 uvG = clamp((p - offset) / uResolution, vec2(0.0), vec2(1.0));
  vec2 uvR = clamp((p - offset * 1.03) / uResolution, vec2(0.0), vec2(1.0));
  vec2 uvB = clamp((p - offset * 0.97) / uResolution, vec2(0.0), vec2(1.0));
  vec3 col;
  col.r = texture2D(uTexture, uvR).r;
  col.g = texture2D(uTexture, uvG).g;
  col.b = texture2D(uTexture, uvB).b;
  float edge = smoothstep(0.0, uEdge, min(min(p.x, uResolution.x - p.x), min(p.y, uResolution.y - p.y)));
  float alpha = clamp(uStrength * 1.6, 0.0, 1.0) * uOpacity * band * edge;
  gl_FragColor = vec4(col * alpha, alpha);
}
`;
