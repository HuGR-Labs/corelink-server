export interface ExtractOptions {
  dir: string
  defaultDirMode?: string | number
  defaultFileMode?: string | number
  onEntry?: (entry: unknown, zipfile: unknown) => void
}

declare function extract(zipPath: string, options: ExtractOptions): Promise<void>
export = extract
