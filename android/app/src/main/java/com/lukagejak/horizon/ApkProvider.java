package com.lukagejak.horizon;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileNotFoundException;

/**
 * Hands the downloaded update to Android's package installer, which can't
 * read the app's own files: the one APK in the cache folder, read only.
 */
public final class ApkProvider extends ContentProvider {
    static final String AUTHORITY = "com.lukagejak.horizon.update";
    static final String NAME = "update.apk";

    static Uri uri() {
        return Uri.parse("content://" + AUTHORITY + "/" + NAME);
    }

    private File file() {
        return new File(new File(getContext().getFilesDir(), "cache"), NAME);
    }

    @Override
    public boolean onCreate() {
        return true;
    }

    @Override
    public String getType(Uri uri) {
        return "application/vnd.android.package-archive";
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!NAME.equals(uri.getLastPathSegment()) || !"r".equals(mode)) {
            throw new FileNotFoundException(uri.toString());
        }
        return ParcelFileDescriptor.open(file(), ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection, String[] args, String order) {
        MatrixCursor cursor = new MatrixCursor(
                new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE});
        cursor.addRow(new Object[] {NAME, file().length()});
        return cursor;
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        return null;
    }

    @Override
    public int delete(Uri uri, String selection, String[] args) {
        return 0;
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] args) {
        return 0;
    }
}
