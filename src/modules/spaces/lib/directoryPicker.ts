function normalize(path: string): string {
  return path.replace(/\\/g, "/");
}

export function joinDirectory(parent: string, child: string): string {
  return `${normalize(parent).replace(/\/+$/, "")}/${child}`;
}

export function parentDirectory(path: string): string {
  const normalized = normalize(path);
  if (normalized === "/" || /^[A-Za-z]:\/$/.test(normalized)) {
    return normalized;
  }

  const trimmed = normalized.replace(/\/+$/, "");
  const separator = trimmed.lastIndexOf("/");
  if (separator <= 0) return "/";
  if (separator === 2 && /^[A-Za-z]:/.test(trimmed))
    return `${trimmed.slice(0, 2)}/`;
  return trimmed.slice(0, separator);
}
