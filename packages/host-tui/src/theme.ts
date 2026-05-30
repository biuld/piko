import type { EditorTheme, MarkdownTheme } from "@earendil-works/pi-tui";
import chalk from "chalk";

export function getEditorTheme(): EditorTheme {
  return {
    borderColor: (s: string) => chalk.dim(s),
    selectList: {
      selectedPrefix: (s: string) => chalk.cyan(s),
      selectedText: (s: string) => chalk.cyan(s),
      description: (s: string) => chalk.dim(s),
      scrollInfo: (s: string) => chalk.dim(s),
      noMatch: (s: string) => chalk.red(s),
    },
  };
}

export function getMarkdownTheme(): MarkdownTheme {
  return {
    heading: (s: string) => chalk.bold(s),
    link: (s: string) => chalk.underline.blue(s),
    linkUrl: (s: string) => chalk.dim(s),
    code: (s: string) => chalk.yellow(s),
    codeBlock: (s: string) => chalk.yellow(s),
    codeBlockBorder: (s: string) => chalk.dim(s),
    quote: (s: string) => chalk.italic.dim(s),
    quoteBorder: (s: string) => chalk.dim(s),
    hr: (s: string) => chalk.dim(s),
    listBullet: (s: string) => chalk.dim(s),
    bold: (s: string) => chalk.bold(s),
    italic: (s: string) => chalk.italic(s),
    strikethrough: (s: string) => chalk.strikethrough(s),
    underline: (s: string) => chalk.underline(s),
  };
}
