export interface ImageSize {
  width: number
  height: number
  type?: string
}

declare function imageSize(input: Uint8Array): ImageSize
export {imageSize}
export default imageSize
