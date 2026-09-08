// SPDX-License-Identifier: MIT
'use strict'

const fs = require('fs')
const imageSize = require('./index.cjs')

async function imageSizeFromFile(filePath) {
  const stat = await fs.promises.stat(filePath)
  if (stat.size > imageSize.MAX_INPUT_BYTES) {
    throw new TypeError('Unsupported or unsafe image format: input exceeds 10 MiB')
  }
  return imageSize(await fs.promises.readFile(filePath))
}

module.exports = {imageSizeFromFile}
