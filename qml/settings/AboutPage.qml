// SPDX-License-Identifier: GPL-3.0-or-later
// Standard Kirigami About page plus an Updates row.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.AboutPage {
    id: page

    required property var settings
    // "" (nothing yet), "discover", "releases" or "failed".
    property string updateState: ""

    title: qsTr("About")
    // The desktop file id starts with "org.kde.", which would otherwise add
    // KDE's own Get Involved and Donate links.
    getInvolvedUrl: ""
    donateUrl: ""

    aboutData: ({
        displayName: qsTr("Plasma Visual Screensaver"),
        productName: "plasma-visual-screensaver",
        componentName: "plasma-visual-screensaver",
        programLogo: "org.kde.plasmavisualscreensaver",
        shortDescription: qsTr("Decorative screensaver for Plasma on Wayland. It doesn't lock your session."),
        homepage: "https://github.com/StantonMatt/plasma-wayland-screensaver",
        bugAddress: "https://github.com/StantonMatt/plasma-wayland-screensaver/issues",
        version: page.settings.controller.applicationVersion,
        otherText: "",
        authors: [ { name: "Matthew Stanton", task: "", emailAddress: "", webAddress: "", ocsUsername: "" } ],
        credits: [],
        translators: [],
        licenses: [ { name: "GNU General Public License v3.0 or later", text: page.settings.controller.applicationLicenseText, spdx: "GPL-3.0-or-later" } ],
        copyrightStatement: "© Matthew Stanton",
        desktopFileName: "org.kde.plasmavisualscreensaver"
    })

    Kirigami.Heading {
        Kirigami.FormData.isSection: true
        text: qsTr("Updates")
    }

    RowLayout {
        Layout.leftMargin: Kirigami.Units.gridUnit
        spacing: Kirigami.Units.largeSpacing
        QQC2.Button {
            text: qsTr("Check for Updates")
            icon.name: "system-software-update"
            onClicked: page.updateState = page.settings.controller.openUpdateCenter()
        }
    }

    Kirigami.InlineMessage {
        id: updateMessage
        Layout.leftMargin: Kirigami.Units.gridUnit
        Layout.fillWidth: true
        Layout.maximumWidth: Kirigami.Units.gridUnit * 28
        // The close button overwrites visible; state changes must reapply it.
        Binding {
            target: updateMessage
            property: "visible"
            value: page.updateState !== ""
        }
        type: page.updateState === "failed" ? Kirigami.MessageType.Error : Kirigami.MessageType.Positive
        text: page.updateState === "failed"
              ? qsTr("Couldn't open Discover or the release page.")
              : page.updateState === "discover"
                ? qsTr("Discover is open. Updates for this app appear there.")
                : qsTr("The GitHub release page is open. Download the latest version there.")
        showCloseButton: true
        onVisibleChanged: if (!visible) page.updateState = ""
    }
}
