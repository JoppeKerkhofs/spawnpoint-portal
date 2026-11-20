export const PICTURE_SIZE = 64;

export function coverRect(w: number, h: number): { sx: number; sy: number; size: number } {
  const size = Math.min(w, h);
  return { sx: (w - size) / 2, sy: (h - size) / 2, size };
}

export const shownPicture = (picture: string | null | undefined, failed: boolean) => (picture && !failed ? picture : null);

export async function pictureFromFile(file: File): Promise<string> {
  const bitmap = await createImageBitmap(file);
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = PICTURE_SIZE;
  const { sx, sy, size } = coverRect(bitmap.width, bitmap.height);
  canvas.getContext("2d")!.drawImage(bitmap, sx, sy, size, size, 0, 0, PICTURE_SIZE, PICTURE_SIZE);
  bitmap.close();
  return canvas.toDataURL("image/png");
}
