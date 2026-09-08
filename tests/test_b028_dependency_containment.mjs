import assert from 'node:assert/strict'
import {readdirSync} from 'node:fs'
import {mkdtemp, readFile, rm, writeFile} from 'node:fs/promises'
import {tmpdir} from 'node:os'
import path from 'node:path'
import {createRequire} from 'node:module'
import test from 'node:test'

import imageSize from '../vendor/image-size/index.mjs'
import {imageSizeFromFile} from '../vendor/image-size/fromFile.mjs'

const require = createRequire(import.meta.url)
const virtualStore = path.resolve('node_modules/.pnpm')
const localExtractPackage = readdirSync(virtualStore).find(name => name.startsWith('extract-zip@file+vendor+extract-zip'))
assert.ok(localExtractPackage, 'frozen pnpm install must link the local extract-zip package')
const extract = require(path.join(virtualStore, localExtractPackage, 'node_modules/extract-zip'))

function crc32(bytes) {
  let crc = 0xffffffff
  for (const byte of bytes) {
    crc ^= byte
    for (let bit = 0; bit < 8; bit++) {
      crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0)
    }
  }
  return (crc ^ 0xffffffff) >>> 0
}

function zip(entries) {
  const locals = []
  const centrals = []
  let offset = 0
  for (const entry of entries) {
    const name = Buffer.from(entry.name)
    const data = Buffer.from(entry.data)
    const local = Buffer.alloc(30 + name.length + data.length)
    local.writeUInt32LE(0x04034b50, 0)
    local.writeUInt16LE(20, 4)
    local.writeUInt16LE(0, 6)
    local.writeUInt16LE(0, 8) // stored, so the fixture needs no compressor
    local.writeUInt16LE(0, 10)
    local.writeUInt16LE(0, 12)
    local.writeUInt32LE(crc32(data), 14)
    local.writeUInt32LE(data.length, 18)
    local.writeUInt32LE(data.length, 22)
    local.writeUInt16LE(name.length, 26)
    local.writeUInt16LE(0, 28)
    name.copy(local, 30)
    data.copy(local, 30 + name.length)
    locals.push(local)

    const central = Buffer.alloc(46 + name.length)
    central.writeUInt32LE(0x02014b50, 0)
    central.writeUInt16LE((3 << 8) | 20, 4) // Unix creator, ZIP 2.0
    central.writeUInt16LE(20, 6)
    central.writeUInt16LE(0, 8)
    central.writeUInt16LE(0, 10)
    central.writeUInt16LE(0, 12)
    central.writeUInt16LE(0, 14)
    central.writeUInt32LE(crc32(data), 16)
    central.writeUInt32LE(data.length, 20)
    central.writeUInt32LE(data.length, 24)
    central.writeUInt16LE(name.length, 28)
    central.writeUInt16LE(0, 30)
    central.writeUInt16LE(0, 32)
    central.writeUInt16LE(0, 34)
    central.writeUInt16LE(0, 36)
    central.writeUInt32LE((entry.mode ?? 0) >>> 0, 38)
    central.writeUInt32LE(offset, 42)
    name.copy(central, 46)
    centrals.push(central)
    offset += local.length
  }
  const central = Buffer.concat(centrals)
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0)
  end.writeUInt16LE(entries.length, 8)
  end.writeUInt16LE(entries.length, 10)
  end.writeUInt32LE(central.length, 12)
  end.writeUInt32LE(offset, 16)
  return Buffer.concat([...locals, central, end])
}

test('image-size accepts bounded docs formats and rejects vulnerable paths', async () => {
  const png = Buffer.alloc(24)
  Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]).copy(png)
  png.writeUInt32BE(320, 16)
  png.writeUInt32BE(200, 20)
  assert.deepEqual(imageSize(png), {width: 320, height: 200, type: 'png'})
  assert.deepEqual(imageSize(Buffer.from('<svg width="12px" height="8px"></svg>')), {
    width: 12,
    height: 8,
    type: 'svg',
  })
  assert.throws(() => imageSize(Buffer.from('icns\0\0\0\x08')), /Unsupported or unsafe/)
  assert.throws(() => imageSize(Buffer.alloc(10 * 1024 * 1024 + 1)), /10 MiB/)

  const root = await mkdtemp(path.join(tmpdir(), 'b028-image-'))
  try {
    const file = path.join(root, 'asset.svg')
    await writeFile(file, '<svg viewBox="0 0 16 9"></svg>')
    assert.deepEqual(await imageSizeFromFile(file), {width: 16, height: 9, type: 'svg'})
  } finally {
    await rm(root, {recursive: true, force: true})
  }
})

test('extract-zip rejects traversal and escaping symlinks before writing', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'b028-zip-'))
  const archive = path.join(root, 'malicious.zip')
  const outside = path.join(root, '..', 'outside-b028.txt')
  try {
    await writeFile(archive, zip([{name: 'nested/ok.txt', data: 'safe'}]))
    const seen = []
    const normalOut = path.join(root, 'normal')
    await extract(archive, {dir: normalOut, onEntry: entry => seen.push(entry.fileName)})
    assert.deepEqual(seen, ['nested/ok.txt'])
    assert.equal(await readFile(path.join(normalOut, 'nested/ok.txt'), 'utf8'), 'safe')
    await assert.rejects(() => extract(archive, {dir: normalOut}), /EEXIST/)

    await writeFile(archive, zip([
      {name: 'escape', data: '../../outside-b028.txt', mode: 0o120777 << 16},
    ]))
    await assert.rejects(() => extract(archive, {dir: path.join(root, 'out')}), /symlink target|Out of bound/)
    await assert.rejects(() => extract(archive, {dir: path.join(root, 'out2')}), /symlink target|Out of bound/)

    await writeFile(archive, zip([{name: '../outside-b028.txt', data: 'escape'}]))
    await assert.rejects(() => extract(archive, {dir: path.join(root, 'out3')}), /Out of bound|invalid relative path/)
    await assert.rejects(() => readFile(outside), /ENOENT/)
  } finally {
    await rm(root, {recursive: true, force: true})
    await rm(outside, {force: true})
  }
})
