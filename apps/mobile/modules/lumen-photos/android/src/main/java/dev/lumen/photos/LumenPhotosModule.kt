package dev.lumen.photos

import android.content.ContentUris
import android.os.Bundle
import android.provider.MediaStore
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

/**
 * The photo library for JavaScript (ADR-0007): the number of images and pages
 * of them from MediaStore. Pages hold identifiers and content URIs only;
 * nothing is changed. Access is requested from JavaScript (READ_MEDIA_IMAGES).
 */
class LumenPhotosModule : Module() {
  private val resolver
    get() = appContext.reactContext?.contentResolver

  private val collection = MediaStore.Images.Media.EXTERNAL_CONTENT_URI

  override fun definition() = ModuleDefinition {
    Name("LumenPhotos")

    Function("count") {
      resolver
        ?.query(collection, arrayOf(MediaStore.Images.Media._ID), null, null)
        ?.use { it.count } ?: 0
    }

    Function("page") { offset: Int, limit: Int ->
      if (offset < 0 || limit <= 0) {
        return@Function emptyList<Map<String, Any>>()
      }
      val args = Bundle().apply {
        putInt(android.content.ContentResolver.QUERY_ARG_OFFSET, offset)
        putInt(android.content.ContentResolver.QUERY_ARG_LIMIT, limit)
        putStringArray(
          android.content.ContentResolver.QUERY_ARG_SORT_COLUMNS,
          arrayOf(MediaStore.Images.Media.DATE_ADDED),
        )
        putInt(
          android.content.ContentResolver.QUERY_ARG_SORT_DIRECTION,
          android.content.ContentResolver.QUERY_SORT_DIRECTION_DESCENDING,
        )
      }
      val projection = arrayOf(
        MediaStore.Images.Media._ID,
        MediaStore.Images.Media.WIDTH,
        MediaStore.Images.Media.HEIGHT,
      )
      val page = mutableListOf<Map<String, Any>>()
      resolver?.query(collection, projection, args, null)?.use { cursor ->
        val id = cursor.getColumnIndexOrThrow(MediaStore.Images.Media._ID)
        val width = cursor.getColumnIndexOrThrow(MediaStore.Images.Media.WIDTH)
        val height = cursor.getColumnIndexOrThrow(MediaStore.Images.Media.HEIGHT)
        while (cursor.moveToNext()) {
          val itemId = cursor.getLong(id)
          page.add(
            mapOf(
              "id" to itemId.toString(),
              "uri" to ContentUris.withAppendedId(collection, itemId).toString(),
              "width" to cursor.getInt(width),
              "height" to cursor.getInt(height),
            ),
          )
        }
      }
      page
    }
  }
}
