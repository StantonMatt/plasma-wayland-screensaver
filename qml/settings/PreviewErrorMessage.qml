// SPDX-License-Identifier: GPL-3.0-or-later
// Shown at the top of a page when Preview could not start.
import QtQuick
import org.kde.kirigami as Kirigami

Kirigami.InlineMessage {
    id: message
    required property var settings
    position: Kirigami.InlineMessage.Position.Header
    type: Kirigami.MessageType.Error
    // Kirigami's close button assigns visible=false. Keep a Binding object
    // that reapplies visibility when dismissal clears the error state.
    Binding {
        target: message
        property: "visible"
        value: message.settings.previewError.length > 0
    }
    text: settings.previewError
    showCloseButton: true
    onVisibleChanged: if (!visible) settings.previewError = ""
}
