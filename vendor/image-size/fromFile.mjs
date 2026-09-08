import {promises as fs} from 'node:fs'
import imageSize from './index.mjs'

export async function imageSizeFromFile(filePath) {
  const stat = await fs.stat(filePath)
  if (stat.size > imageSize.MAX_INPUT_BYTES) {
    throw new TypeError('Unsupported or unsafe image format: input exceeds 10 MiB')
  }
  return imageSize(await fs.readFile(filePath))
}
