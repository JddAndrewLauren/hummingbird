package net.twinion.hummingbird.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** `ItemLink`'s reading of an arriving intent, over the two strings it
 * reads — and `null` for everything that is not an item link, the
 * notification tap's `hummingbird://alert/<id>` above all, which shares the
 * scheme and the VIEW action and must never be read as one. */
class ItemLinkTest {

    private val view = "android.intent.action.VIEW"

    @Test
    fun `an item link names its item`() {
        assertEquals("0b1c-item", ItemLink.itemId(view, "hummingbird://item/0b1c-item"))
    }

    @Test
    fun `anything that is not an item link answers null`() {
        assertNull(ItemLink.itemId(null, null))
        assertNull(ItemLink.itemId("android.intent.action.MAIN", null))
        assertNull(ItemLink.itemId(view, "hummingbird://alert/a1"))
        assertNull(ItemLink.itemId("android.intent.action.SEND", "hummingbird://item/a1"))
        assertNull(ItemLink.itemId(view, "https://item/a1"))
        assertNull(ItemLink.itemId(view, "hummingbird://item/"))
        assertNull(ItemLink.itemId(view, "hummingbird://item/a1/steps"))
    }
}
