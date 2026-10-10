// SPDX-License-Identifier: GPL-3.0-or-later
// Interaction rules of the settings window (see qml/Settings.qml).
import QtQuick
import QtQuick.Controls as QQC2
import QtTest

TestCase {
    id: tc
    name: "SettingsWindow"
    when: windowShown

    property var win

    SignalSpy { id: saves; target: testConfig; signalName: "saved" }

    function topItem(item) { while (item.parent) item = item.parent; return item }
    function findAll(item, pred, acc) {
        acc = acc || []
        if (pred(item)) acc.push(item)
        const kids = item.children || []
        for (let i = 0; i < kids.length; ++i) findAll(kids[i], pred, acc)
        return acc
    }
    // Kirigami creates toolbar, notification and view items asynchronously.
    function find(pred, root) {
        let found = []
        tryVerify(() => (found = findAll(root || topItem(win.contentItem), pred)).length > 0, 3000, "item not found")
        return found[0]
    }
    function page(id) {
        win.showPage(id)
        return win.pageCache[id]
    }
    function slider(name) {
        return find(it => it.accessibleName === name && it.shownValue !== undefined && it.visible, page("appearance"))
    }
    function label(text) {
        return findAll(page("appearance"), it => it.text === text && it.visible).length > 0
    }
    function gallery() {
        const view = find(it => it.implicitCellWidth !== undefined && it.columns !== undefined, page("appearance"))
        tryVerify(() => view.itemAtIndex(view.count - 1) !== null)
        return view
    }
    function activate() {
        win.requestActivate()
        tryVerify(() => win.active)
    }
    function dismiss(message) {
        const close = find(it => it.text === "Close" && it.clicked !== undefined && it.visible, message)
        tryVerify(() => message.height >= close.height)
        mouseClick(close)
        tryCompare(message, "visible", false)
    }
    function reveal(item, owner) {
        const flick = owner.flickable
        const y = item.mapToItem(flick.contentItem, 0, 0).y
        flick.contentY = Math.max(0, Math.min(y, flick.contentHeight - flick.height))
        waitForRendering(item)
    }

    function init() {
        testConfig.restoreDefaults()
        testConfig.save()
        testController.monitorCount = 3
        testController.updateResult = "discover"
        testController.previews = 0
        testController.quits = 0
        testController.updateChecks = 0
        win = Qt.createComponent("qrc:/qml/Settings.qml").createObject(null, {
            controller: testController, screensaverConfig: testConfig
        })
        verify(win)
        tryVerify(() => win.pageStack.currentItem !== null)
        // A user can't act before the first frame; neither do the tests.
        waitForRendering(win.pageStack.currentItem)
        saves.clear()
    }
    function cleanup() {
        win.destroy()
        win = null
        wait(0)
    }

    function test_inventorySetting() {
        win.write("visualModule", "snakes")
        const appearance = page("appearance")
        const power = find(it => it.text === "Power-ups" && it.visible, appearance)
        const inventory = find(it => it.objectName === "snakeStorePowerUpsBox" && it.visible, appearance)
        compare(inventory.checked, true)
        compare(inventory.enabled, true)
        reveal(inventory, appearance)
        mouseClick(inventory)
        compare(testConfig.snakeStorePowerUps, false)
        win.write("snakePowerUps", false)
        compare(inventory.enabled, false)
        win.write("snakePowerUps", true)
        compare(inventory.enabled, true)
        compare(inventory.checked, false)
        verify(inventory.mapToItem(appearance, 0, 0).x > power.mapToItem(appearance, 0, 0).x)
    }

    function test_instantApplyBatchesSaves() {
        win.write("animationSpeed", 120)
        win.write("animationSpeed", 130)
        win.write("animationSpeed", 140)
        compare(testConfig.animationSpeed, 140)
        compare(saves.count, 0)
        tryCompare(saves, "count", 1, 2000)
        wait(600)
        compare(saves.count, 1)
    }

    function test_sliderKeyboardWritesAndSaves() {
        activate()
        const speed = slider("Speed")
        const handle = find(it => it.stepSize === 10 && it.snapMode !== undefined, speed)
        handle.forceActiveFocus(Qt.TabFocusReason)
        keyClick(Qt.Key_Right)
        compare(testConfig.animationSpeed, 110)
        compare(speed.valueText, "110%")
        tryCompare(saves, "count", 1, 2000)
    }

    function test_clockComboMapsToClockMode() {
        const combo = find(it => it.displayText !== undefined && it.count === 5, page("appearance"))
        compare(combo.displayText, "Drifting")
        win.write("clockMode", 0)
        compare(testConfig.showClock, false)
        compare(combo.displayText, "Off")
        combo.activated(1)
        compare(testConfig.showClock, true)
        compare(testConfig.clockMovement, "center")
        combo.activated(4)
        compare(testConfig.clockMovement, "bounce")
        compare(testConfig.clockSpeed, "fast")
        compare(combo.currentIndex, 4)
    }

    function test_seasonalCheckbox() {
        const appearance = page("appearance")
        const check = find(it => it.objectName === "seasonalThemesCheckBox", appearance)
        win.write("visualModule", "snakes")
        verify(check.visible)
        verify(check.checked)
        check.checked = false
        check.toggled()
        compare(testConfig.seasonalThemes, false)
        win.write("visualModule", "bounce")
        verify(!check.visible)
    }

    function test_optionsFollowSelectedAnimation() {
        page("appearance")
        win.write("visualModule", "snakes")
        verify(label("Population:"))
        verify(label("Food:"))
        verify(label("Intelligence:"))
        verify(label("Rules:"))
        for (const rule of ["Power-ups", "World events", "Snakes can crash into themselves",
                            "Screen edges are deadly", "Limit snake length"])
            verify(label(rule), rule)
        verify(!label("Balls:"))

        win.write("visualModule", "bounce")
        verify(label("Balls:"))
        verify(label("Gravity:"))
        verify(!label("Population:"))
        verify(findAll(page("appearance"), it => it.accessibleName === "" && it.shownValue !== undefined && it.visible).length === 0,
               "Bouncing Balls has no density slider")

        win.write("visualModule", "none")
        verify(!label("Colors:"))
        verify(label("Background:"))
        verify(label("Clock:"))
    }

    function test_worldEventsVisibleWithBindingIntact() {
        win.write("visualModule", "snakes")
        const events = find(it => it.text === "World events" && it.toggled !== undefined, page("appearance"))
        verify(events.visible)
        win.write("snakeWorldEvents", false)
        compare(events.checked, false)
        win.write("snakeWorldEvents", true)
        compare(events.checked, true)
        verify(events.visible)
        events.checked = false
        events.toggled()
        compare(testConfig.snakeWorldEvents, false)
        win.write("visualModule", "aurora")
        verify(!events.visible)
    }

    function test_slidersShowEachAnimationsOwnValues() {
        win.write("visualModule", "snakes")
        win.write("animationSpeed", 250)
        win.write("animationPalette", "ember")
        win.write("visualModule", "aurora")
        compare(slider("Speed").value, 100)
        compare(testConfig.animationPalette, "ocean")
        win.write("visualModule", "snakes")
        compare(slider("Speed").value, 250)
        compare(testConfig.animationPalette, "ember")
    }

    function test_appearanceDefaultsThenUndo() {
        const defaults = page("appearance").actions[0]
        verify(!defaults.enabled, "nothing to restore on a fresh configuration")
        win.write("visualModule", "snakes")
        win.write("animationSpeed", 300)
        win.write("snakeAggression", 40)
        win.write("visualModule", "matrix")
        win.write("trailAmount", 90)
        win.write("idleMinutes", 3)
        verify(defaults.enabled)

        defaults.trigger()
        compare(testConfig.visualModule, "aurora")
        compare(testConfig.snakeAggression, 100)
        compare(testConfig.snapshot("appearance").animationSettings.snakes.animationSpeed, 100)
        compare(testConfig.idleMinutes, 3, "Appearance defaults leave General alone")
        verify(!defaults.enabled)

        const undo = find(it => it.text === "Undo" && it.clicked !== undefined && it.visible)
        mouseClick(undo)
        compare(testConfig.visualModule, "matrix")
        compare(testConfig.trailAmount, 90)
        compare(testConfig.snakeAggression, 40)
        compare(testConfig.snapshot("appearance").animationSettings.snakes.animationSpeed, 300)
        tryCompare(saves, "count", 1, 2000)
    }

    function test_generalDefaultsThenUndo() {
        const defaults = page("general").actions[0]
        verify(!defaults.enabled)
        win.write("idleMinutes", 1)
        win.write("monitorBehavior", "seamless")
        win.write("visualModule", "snakes")
        defaults.trigger()
        compare(testConfig.idleMinutes, 10)
        compare(testConfig.monitorBehavior, "independent")
        compare(testConfig.visualModule, "snakes", "General defaults leave Appearance alone")
        mouseClick(find(it => it.text === "Undo" && it.clicked !== undefined && it.visible))
        compare(testConfig.idleMinutes, 1)
        compare(testConfig.monitorBehavior, "seamless")
    }

    function test_galleryClickAndArrowKeysSelect() {
        activate()
        const view = gallery()
        compare(view.currentIndex, 1)
        mouseClick(view.itemAtIndex(10))
        compare(testConfig.visualModule, "snakes")
        keyClick(Qt.Key_Left)
        compare(testConfig.visualModule, "constellation")
        keyClick(Qt.Key_Up)
        compare(testConfig.visualModule, "bounce")
        compare(testController.previews, 0)
    }

    function test_tilePreviewSelectsSavesAndPreviews() {
        const view = gallery()
        const tile = view.itemAtIndex(3)
        tile.actions[0].trigger()
        compare(testConfig.visualModule, "bounce")
        compare(saves.count, 1, "saved before the preview starts")
        compare(testController.previews, 1)
    }

    function test_previewFailureShowsAndClears() {
        win.write("trailAmount", 70)
        win.preview()
        compare(saves.count, 1)
        compare(testController.previews, 1)
        testController.previewFailed("The display couldn't be prepared.")
        compare(win.previewError, "Couldn't start the preview. The display couldn't be prepared.")
        verify(find(it => it.text === win.previewError && it.visible, page("appearance")))
        win.preview()
        compare(win.previewError, "")
    }

    function test_closeHidesAndFlushes() {
        win.write("trailAmount", 60)
        win.close()
        compare(win.visible, false)
        compare(saves.count, 1)
        compare(testController.quits, 0)
    }

    // Quit over D-Bus destroys the window without closing it first.
    function test_destructionFlushesPendingSave() {
        win.write("trailAmount", 55)
        compare(saves.count, 0)
        win.destroy()
        wait(0)
        compare(saves.count, 1)
        win = Qt.createComponent("qrc:/qml/Settings.qml").createObject(null, {
            controller: testController, screensaverConfig: testConfig
        })
    }

    function test_escapeClosesExceptWhilePopupIsOpen() {
        activate()
        const combo = find(it => it.displayText !== undefined && it.count === 5, page("appearance"))
        combo.forceActiveFocus()
        combo.popup.open()
        tryVerify(() => combo.popup.opened)
        keyClick(Qt.Key_Escape)
        tryVerify(() => !combo.popup.visible)
        verify(win.visible, "Escape closed the list, not the window")
        keyClick(Qt.Key_Escape)
        tryVerify(() => !win.visible)
    }

    function test_monitorChooserOnlyWithSeveralMonitors() {
        const general = page("general")
        const chooser = find(it => it.modes !== undefined, general)
        verify(chooser.visible)
        testController.monitorCount = 1
        verify(!chooser.visible)
        verify(findAll(general, it => it.text === "Panels:" && it.visible).length > 0)
        testController.monitorCount = 2
        verify(chooser.visible)
        mouseClick(find(it => it.text === "Spanning" && it.checkable, chooser))
        compare(testConfig.monitorBehavior, "seamless")
    }

    function test_reducedMotionNoticeAndFrameRate() {
        const general = page("general")
        const frameRate = find(it => it.displayText !== undefined && it.count === 15, general)
        win.write("reducedMotion", true)
        verify(!frameRate.enabled)
        win.write("visualModule", "snakes")
        verify(frameRate.enabled, "Snakes keep their frame rate under reduced motion")
        const note = find(it => it.text === "Slithering Snakes runs at up to 60 fps.", general)
        verify(!note.visible)
        win.write("frameRate", 0)
        verify(note.visible)

        const turnOff = find(it => it.text === "Turn Off" && it.visible, page("appearance"))
        mouseClick(turnOff)
        compare(testConfig.reducedMotion, false)
    }

    function test_stopFlushesThenQuits() {
        win.write("idleMinutes", 7)
        mouseClick(find(it => it.text === "Stop" && it.clicked !== undefined, page("general")))
        compare(saves.count, 1)
        compare(testController.quits, 1)
    }

    function test_swatchesActAsRadioGroup() {
        activate()
        const picker = find(it => it.accessibleName === "Background" && it.currentIndex !== undefined, page("appearance"))
        const selected = find(it => it.checkable && it.checked, picker)
        selected.forceActiveFocus(Qt.TabFocusReason)
        keyClick(Qt.Key_Right)
        compare(testConfig.backgroundStyle, "ocean")
        keyClick(Qt.Key_Left)
        keyClick(Qt.Key_Left)
        compare(testConfig.backgroundStyle, "black")
    }

    function test_choiceRepeatedActivation_data() {
        return [
            { tag: "background", kind: "swatch", name: "Background", key: "backgroundStyle" },
            { tag: "palette", kind: "swatch", name: "Colors", key: "animationPalette" },
            { tag: "monitors", kind: "monitors", key: "monitorBehavior" },
            { tag: "gallery", kind: "gallery", key: "visualModule" }
        ]
    }
    function test_choiceRepeatedActivation(data) {
        activate()
        const owner = page(data.kind === "monitors" ? "general" : "appearance")
        const group = data.kind === "gallery" ? gallery()
                    : data.kind === "monitors" ? find(it => it.modes !== undefined, owner)
                    : find(it => it.accessibleName === data.name && it.currentIndex !== undefined, owner)
        const selected = data.kind === "gallery" ? group.itemAtIndex(group.currentIndex)
                       : find(it => it.checkable && it.checked, group)
        const value = testConfig[data.key]
        reveal(selected, owner)
        for (let i = 0; i < 2; ++i) {
            mouseClick(selected, selected.width / 2, selected.height * 0.75)
            verify(selected.checked, "selected choice stays checked after a click")
            selected.forceActiveFocus(Qt.TabFocusReason)
            keyClick(Qt.Key_Space)
            verify(selected.checked, "selected choice stays checked after Space")
            verify(selected.Accessible.checked)
            compare(findAll(group, it => it.checkable && it.checked).length, 1)
            verify(data.kind === "gallery" ? group.activeFocusOnTab : selected.activeFocusOnTab)
            compare(testConfig[data.key], value)
        }
        compare(saves.count, 0, "reselecting does not schedule a save")
        compare(testController.previews, 0, "selecting does not start a preview")
        keyClick(Qt.Key_Right)
        verify(testConfig[data.key] !== value, "arrow keys still select another choice")
        keyClick(Qt.Key_Left)
        compare(testConfig[data.key], value)
        verify(selected.checked)
        compare(findAll(group, it => it.checkable && it.checked).length, 1)
    }

    function test_previewErrorReopensAfterDismissal_data() {
        return [{ tag: "appearance", pageId: "appearance" }, { tag: "general", pageId: "general" }]
    }
    function test_previewErrorReopensAfterDismissal(data) {
        const owner = page(data.pageId)
        for (const reason of ["First failure", "First failure", "Another failure"]) {
            testController.previewFailed(reason)
            const message = find(it => it.showCloseButton === true && it.text === win.previewError, owner)
            tryCompare(message, "visible", true)
            dismiss(message)
            compare(win.previewError, "")
        }
        page("about")
        page(data.pageId)
        testController.previewFailed("Failure after revisiting")
        verify(find(it => it.showCloseButton === true && it.text === win.previewError && it.visible, owner))
    }

    function test_narrowWindowCollapsesSidebar() {
        const general = find(it => it.text === "General" && it.display !== undefined)
        compare(general.display, QQC2.AbstractButton.TextBesideIcon)
        compare(gallery().columns, 6)
        win.width = win.minimumWidth
        tryVerify(() => win.narrow)
        compare(general.display, QQC2.AbstractButton.IconOnly)
        tryCompare(gallery(), "columns", 4)
    }

    function test_checkForUpdatesReportsResult() {
        const about = page("about")
        compare(about.aboutData.version, "9.9.9")
        const button = find(it => it.text === "Check for Updates", about)
        mouseClick(button)
        compare(testController.updateChecks, 1)
        verify(find(it => it.text === "Discover is open. Updates for this app appear there." && it.visible, about))
        testController.updateResult = "releases"
        mouseClick(button)
        verify(find(it => it.text === "The GitHub release page is open. Download the latest version there." && it.visible, about))
        verify(!findAll(about, it => it.text === "Discover is open. Updates for this app appear there." && it.visible).length)
        testController.updateResult = "failed"
        mouseClick(button)
        compare(testController.updateChecks, 3)
        verify(find(it => it.text === "Couldn't open Discover or the release page." && it.visible, about))
    }

    function test_updateResultReopensAfterDismissal() {
        const about = page("about")
        const button = find(it => it.text === "Check for Updates", about)
        const message = find(it => it.showCloseButton === true, about)
        for (const result of ["discover", "discover", "releases", "failed", "failed", "discover"]) {
            testController.updateResult = result
            mouseClick(button)
            compare(about.updateState, result)
            tryCompare(message, "visible", true)
            dismiss(message)
            compare(about.updateState, "")
        }
        compare(testController.updateChecks, 6)
        page("general")
        page("about")
        mouseClick(button)
        tryCompare(message, "visible", true)
    }

    function test_licenseLinkShowsFullText() {
        const about = page("about")
        const license = about.aboutData.licenses[0]
        verify(license.text.indexOf("GNU GENERAL PUBLIC LICENSE") >= 0)
        verify(license.text.indexOf("END OF TERMS AND CONDITIONS") >= 0)
        verify(license.text.indexOf("How to Apply These Terms to Your New Programs") >= 0)
        const link = find(it => it.text === license.name && it.clicked !== undefined, about)
        reveal(link, about)
        mouseClick(link)
        verify(find(it => it.text === license.text && it.visible))
    }
}
