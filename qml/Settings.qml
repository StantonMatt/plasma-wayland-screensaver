// SPDX-License-Identifier: GPL-3.0-or-later
// Settings window: a sidebar with Appearance, General and About. Every control
// writes its Configuration property directly; the file write is batched.
pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.delegates as Delegates

Kirigami.ApplicationWindow {
    id: window

    required property var controller
    required property var screensaverConfig
    property string initialPage: "appearance"
    // Set when a preview could not start; shown at the top of the page.
    property string previewError: ""

    width: Kirigami.Units.gridUnit * 57
    height: Kirigami.Units.gridUnit * 40
    minimumWidth: Kirigami.Units.gridUnit * 34
    minimumHeight: Kirigami.Units.gridUnit * 26
    visible: true
    title: qsTr("Plasma Visual Screensaver")

    // Hide instead of quitting; the background service keeps running.
    onClosing: close => {
        close.accepted = false
        window.flush()
        window.hide()
    }
    // Quit over D-Bus destroys the window without closing it.
    Component.onDestruction: flush()

    // Open popups (combo lists, menus) take Escape first.
    Shortcut {
        sequences: [StandardKey.Close, "Escape"]
        onActivated: window.close()
    }

    // ---- Instant apply ---------------------------------------------------
    Timer {
        id: saveTimer
        interval: 400
        onTriggered: window.screensaverConfig.save()
    }

    function write(key, value) {
        if (screensaverConfig[key] === value) return
        screensaverConfig[key] = value
        saveTimer.restart()
    }

    function flush() {
        if (!saveTimer.running) return
        saveTimer.stop()
        screensaverConfig.save()
    }

    // Defaults stay disabled while the page already matches them, as in
    // System Settings. Re-evaluated on every Configuration change.
    property int revision: 0
    Connections {
        target: window.screensaverConfig
        function onChanged() { window.revision += 1 }
    }
    readonly property string appearanceDefaultsJson: JSON.stringify(screensaverConfig.defaults("appearance"))
    readonly property string generalDefaultsJson: JSON.stringify(screensaverConfig.defaults("general"))
    readonly property bool appearanceIsDefault: revision >= 0
        && JSON.stringify(screensaverConfig.snapshot("appearance")) === appearanceDefaultsJson
    readonly property bool generalIsDefault: revision >= 0
        && JSON.stringify(screensaverConfig.snapshot("general")) === generalDefaultsJson

    function restoreDefaults(page) {
        const previous = screensaverConfig.snapshot(page)
        screensaverConfig.restoreDefaults(page)
        saveTimer.restart()
        showPassiveNotification(page === "general" ? qsTr("General settings restored to defaults")
                                                   : qsTr("Appearance restored to defaults"),
                                "long", qsTr("Undo"), () => {
            screensaverConfig.apply(previous)
            saveTimer.restart()
        })
    }

    function preview() {
        previewError = ""
        saveTimer.stop()
        screensaverConfig.save()
        controller.Preview()
    }

    function quit() {
        saveTimer.stop()
        screensaverConfig.save()
        controller.Quit()
    }

    Connections {
        target: window.controller
        function onPreviewFailed(reason) {
            window.previewError = qsTr("Couldn't start the preview. %1").arg(reason)
        }
    }

    // ---- Pages -----------------------------------------------------------
    readonly property var pages: [
        { id: "appearance", text: qsTr("Appearance"), icon: "preferences-desktop-theme-global", source: "settings/AppearancePage.qml" },
        { id: "general", text: qsTr("General"), icon: "preferences-system", source: "settings/GeneralPage.qml" },
        { id: "about", text: qsTr("About"), icon: "help-about", source: "settings/AboutPage.qml" }
    ]
    property var pageCache: ({})
    property string currentPage: ""

    function showPage(id) {
        if (id === currentPage) return
        let page = pageCache[id]
        if (!page) {
            const entry = pages.find(p => p.id === id)
            const component = Qt.createComponent(entry.source)
            if (component.status !== Component.Ready) {
                console.error(component.errorString())
                return
            }
            page = component.createObject(window, { settings: window })
            pageCache[id] = page
        }
        currentPage = id
        if (pageStack.depth === 0) pageStack.push(page)
        else pageStack.replace(page)
    }

    Component.onCompleted: showPage(initialPage)

    pageStack.globalToolBar.style: Kirigami.ApplicationHeaderStyle.ToolBar
    pageStack.globalToolBar.showNavigationButtons: Kirigami.ApplicationHeaderStyle.NoNavigationButtons
    pageStack.columnView.columnResizeMode: Kirigami.ColumnView.SingleColumn

    // ---- Sidebar ---------------------------------------------------------
    // Same construction as Kirigami Addons' ConfigurationView window, minus
    // its search header. Collapses to icons on narrow windows.
    readonly property bool narrow: width < Kirigami.Units.gridUnit * 40

    globalDrawer: Kirigami.OverlayDrawer {
        edge: Application.layoutDirection === Qt.RightToLeft ? Qt.RightEdge : Qt.LeftEdge
        modal: false
        drawerOpen: true
        handleVisible: false
        width: window.narrow ? Kirigami.Units.gridUnit * 3 : Kirigami.Units.gridUnit * 10
        Kirigami.Theme.colorSet: Kirigami.Theme.View
        Kirigami.Theme.inherit: false
        leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0

        contentItem: ListView {
            id: sidebar
            topMargin: Kirigami.Units.smallSpacing
            model: window.pages
            currentIndex: window.pages.findIndex(p => p.id === window.currentPage)
            activeFocusOnTab: true
            Accessible.role: Accessible.PageTabList

            delegate: Delegates.RoundedItemDelegate {
                required property var modelData
                required property int index
                text: modelData.text
                icon.name: modelData.icon
                checked: ListView.isCurrentItem
                display: window.narrow ? QQC2.AbstractButton.IconOnly : QQC2.AbstractButton.TextBesideIcon
                Accessible.role: Accessible.PageTab
                QQC2.ToolTip.text: text
                QQC2.ToolTip.visible: window.narrow && hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: window.showPage(modelData.id)
            }
            Keys.onUpPressed: window.showPage(window.pages[Math.max(0, currentIndex - 1)].id)
            Keys.onDownPressed: window.showPage(window.pages[Math.min(count - 1, currentIndex + 1)].id)
        }
    }
}
