package dev.lumen.core

import android.content.ContentResolver
import android.content.ContentUris
import android.graphics.Bitmap
import android.graphics.Color
import android.os.Bundle
import android.provider.MediaStore
import android.util.Size

/**
 * Finds near-duplicate photos (ADR-0007's spike, ADR-0019): MediaStore
 * thumbnails → a 64-bit difference hash per photo, as a 0/1 vector → grouping
 * in Rust (Euclidean distance on bits is the square root of the Hamming
 * distance). Android has no Vision; the hash is the fallback ADR-0019 names.
 */
internal object Analyzer {
  private val collection = MediaStore.Images.Media.EXTERNAL_CONTENT_URI

  private fun luminance(pixel: Int): Int =
    (Color.red(pixel) * 299 + Color.green(pixel) * 587 + Color.blue(pixel) * 114) / 1000

  /** The difference hash of `bitmap`: 8 × 8 left-brighter-than-right bits. */
  private fun dHash(bitmap: Bitmap): List<Float> {
    val small = Bitmap.createScaledBitmap(bitmap, 9, 8, true)
    val bits = ArrayList<Float>(64)
    for (y in 0 until 8) {
      for (x in 0 until 8) {
        val left = luminance(small.getPixel(x, y))
        val right = luminance(small.getPixel(x + 1, y))
        bits.add(if (left > right) 1f else 0f)
      }
    }
    return bits
  }

  fun analyze(resolver: ContentResolver, limit: Int, threshold: Float): Map<String, Any> {
    val args = Bundle().apply {
      putInt(ContentResolver.QUERY_ARG_LIMIT, limit.coerceAtLeast(0))
      putStringArray(ContentResolver.QUERY_ARG_SORT_COLUMNS, arrayOf(MediaStore.Images.Media.DATE_ADDED))
      putInt(ContentResolver.QUERY_ARG_SORT_DIRECTION, ContentResolver.QUERY_SORT_DIRECTION_DESCENDING)
    }
    val ids = mutableListOf<String>()
    val prints = mutableListOf<List<Float>>()
    val started = System.nanoTime()
    resolver.query(collection, arrayOf(MediaStore.Images.Media._ID), args, null)?.use { cursor ->
      val column = cursor.getColumnIndexOrThrow(MediaStore.Images.Media._ID)
      while (cursor.moveToNext()) {
        val id = cursor.getLong(column)
        val uri = ContentUris.withAppendedId(collection, id)
        val thumbnail = runCatching { resolver.loadThumbnail(uri, Size(64, 64), null) }.getOrNull()
          ?: continue
        ids.add(id.toString())
        prints.add(dHash(thumbnail))
      }
    }
    val printed = System.nanoTime()
    val groups = uniffi.lumen_ffi.groupFeaturePrints(prints, threshold)
    val members = linkedMapOf<UInt, MutableList<String>>()
    ids.zip(groups).forEach { (id, group) -> members.getOrPut(group) { mutableListOf() }.add(id) }
    return mapOf(
      "analyzed" to ids.size,
      "groups" to members.values.filter { it.size > 1 },
      "printMs" to (printed - started) / 1_000_000,
      "groupMs" to (System.nanoTime() - printed) / 1_000_000,
      "cancelled" to false,
    )
  }
}
