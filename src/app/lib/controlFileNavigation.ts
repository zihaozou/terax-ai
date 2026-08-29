type EditorNavigationHandle = {
  focus(): void;
  gotoLine(line: number, options: { focus: boolean }): void;
};

type ControlFileNavigationDeps = {
  activate: () => Promise<boolean>;
  getEditor: () => EditorNavigationHandle | null;
  setPending: (
    tabId: number,
    navigation: { line?: number; focus: boolean },
  ) => void;
  tabId: number;
  line?: number;
};

export async function activateControlFileNavigation({
  activate,
  getEditor,
  setPending,
  tabId,
  line,
}: ControlFileNavigationDeps): Promise<void> {
  if (!(await activate())) return;
  const editor = getEditor();
  if (!editor) {
    setPending(
      tabId,
      line === undefined ? { focus: true } : { line, focus: true },
    );
    return;
  }
  if (line === undefined) editor.focus();
  else editor.gotoLine(line, { focus: true });
}
