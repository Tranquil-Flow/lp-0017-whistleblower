import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import QtQuick.Dialogs

// Whistleblower (LP-0017) — censorship-resistant document upload + indexing.
// Expects a `backend` context property of type WhistleblowerBackend.

Rectangle {
    id: root
    width: 520
    height: 720
    color: "#0f1117"

    // ── Palette ───────────────────────────────────────────────────────────────
    readonly property color colBg:       "#0f1117"
    readonly property color colSurface:  "#1a1d27"
    readonly property color colBorder:   "#2d3148"
    readonly property color colPrimary:  "#7c6ef5"
    readonly property color colSuccess:  "#3ecf8e"
    readonly property color colWarning:  "#f5a623"
    readonly property color colError:    "#e05252"
    readonly property color colText:     "#e8e9f0"
    readonly property color colMuted:    "#6b7280"
    readonly property int   radius:      12

    // ── Toast notification ────────────────────────────────────────────────────
    Rectangle {
        id: toast
        anchors { bottom: parent.bottom; horizontalCenter: parent.horizontalCenter; bottomMargin: 24 }
        width: toastLabel.implicitWidth + 32
        height: 40
        radius: 20
        color: toastSuccess ? root.colSuccess : root.colError
        opacity: 0
        z: 10

        property bool toastSuccess: true

        Label {
            id: toastLabel
            anchors.centerIn: parent
            color: "#fff"
            font.pixelSize: 13
        }

        SequentialAnimation {
            id: toastAnim
            NumberAnimation { target: toast; property: "opacity"; to: 1; duration: 200 }
            PauseAnimation { duration: 3000 }
            NumberAnimation { target: toast; property: "opacity"; to: 0; duration: 400 }
        }

        function show(msg, success) {
            toast.toastSuccess = success
            toastLabel.text = msg
            toastAnim.restart()
        }
    }

    Connections {
        target: backend
        function onUploadComplete(cid) {
            toast.show("✓ Uploaded — CID " + cid.substring(0, 14) + "…", true)
        }
        function onBroadcastComplete(messageHash) {
            toast.show("✓ Broadcast — " + messageHash.substring(0, 12) + "…", true)
        }
        function onAnchorComplete(cidHash) {
            toast.show("✓ Anchored on chain — " + cidHash.substring(0, 12) + "…", true)
        }
        function onError(stage, msg) {
            toast.show("✗ " + stage + ": " + msg, false)
        }
    }

    FileDialog {
        id: filePicker
        title: "Select document to publish"
        onAccepted: backend.setSelectedFile(filePicker.selectedFile.toString())
    }

    // ── Main layout ───────────────────────────────────────────────────────────
    ColumnLayout {
        anchors { fill: parent; margins: 20 }
        spacing: 16

        // Header
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: "Whistleblower"
                color: root.colPrimary
                font { pixelSize: 22; bold: true }
            }
            Item { Layout.fillWidth: true }
            Label {
                text: "LP-0017"
                color: root.colMuted
                font.pixelSize: 11
            }
        }

        // Stage indicator
        RowLayout {
            Layout.fillWidth: true
            spacing: 4
            Repeater {
                model: ["Pick", "Upload", "Broadcast", "Anchor"]
                delegate: Rectangle {
                    Layout.fillWidth: true
                    height: 6
                    radius: 3
                    color: backend.stage > index ? root.colSuccess
                         : backend.stage === index ? root.colPrimary
                         : root.colBorder
                    Behavior on color { ColorAnimation { duration: 200 } }
                }
            }
        }

        // ── File panel ────────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: fileColumn.implicitHeight + 32
            radius: root.radius
            color: root.colSurface
            border.color: root.colBorder

            ColumnLayout {
                id: fileColumn
                anchors { fill: parent; margins: 16 }
                spacing: 12

                Label {
                    text: "Document"
                    color: root.colMuted
                    font.pixelSize: 11
                    font.capitalization: Font.AllUppercase
                    font.letterSpacing: 1
                }

                RowLayout {
                    spacing: 8
                    WbButton {
                        text: backend.selectedFile === "" ? "Pick file…" : "Change…"
                        accent: backend.selectedFile === ""
                        Layout.preferredWidth: 110
                        onClicked: filePicker.open()
                    }
                    Label {
                        Layout.fillWidth: true
                        text: backend.selectedFile === "" ? "(no file selected)" : backend.selectedFile
                        color: backend.selectedFile === "" ? root.colMuted : root.colText
                        font.pixelSize: 12
                        elide: Text.ElideMiddle
                    }
                }

                // Metadata fields
                WbTextField { id: titleField;       placeholderText: "Title" }
                WbTextField { id: descriptionField; placeholderText: "Description" }
                WbTextField { id: tagsField;        placeholderText: "Tags (comma-separated)" }
            }
        }

        // ── Action panel ──────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: actionColumn.implicitHeight + 32
            radius: root.radius
            color: root.colSurface
            border.color: root.colBorder

            ColumnLayout {
                id: actionColumn
                anchors { fill: parent; margins: 16 }
                spacing: 12

                WbButton {
                    text: "Publish (upload + broadcast)"
                    accent: true
                    enabled: !backend.busy && backend.selectedFile !== "" && titleField.text !== ""
                    onClicked: backend.publish(titleField.text.trim(), descriptionField.text.trim(), tagsField.text.trim())
                }

                Rectangle { height: 1; Layout.fillWidth: true; color: root.colBorder; visible: backend.lastCid !== "" }

                ColumnLayout {
                    visible: backend.lastCid !== ""
                    spacing: 6
                    Label {
                        text: "Last published"
                        color: root.colMuted
                        font.pixelSize: 11
                        font.capitalization: Font.AllUppercase
                        font.letterSpacing: 1
                    }
                    Label {
                        Layout.fillWidth: true
                        text: backend.lastCid
                        color: root.colText
                        font { pixelSize: 12; family: "Menlo, monospace" }
                        elide: Text.ElideMiddle
                    }
                    WbButton {
                        text: backend.lastAnchorTimestamp > 0 ? "Anchored ✓" : "Anchor on chain"
                        accent: backend.lastAnchorTimestamp === 0
                        enabled: !backend.busy && backend.lastAnchorTimestamp === 0
                        onClicked: backend.anchorLast()
                    }
                }
            }
        }

        // ── Status bar ────────────────────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            visible: backend.busy || backend.lastError !== "" || backend.lastCid !== ""

            BusyIndicator {
                width: 20; height: 20
                running: backend.busy
                visible: backend.busy
                palette.dark: root.colPrimary
            }

            Label {
                text: backend.busy ? "Working: " + backend.busyMessage
                    : backend.lastError !== "" ? "Error: " + backend.lastError
                    : "Idle"
                color: backend.lastError !== "" ? root.colError : root.colMuted
                font.pixelSize: 12
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
        }

        Item { Layout.fillHeight: true }
    }

    // ── Shared component definitions ──────────────────────────────────────────

    component WbTextField: TextField {
        Layout.fillWidth: true
        color: root.colText
        placeholderTextColor: root.colMuted
        font.pixelSize: 14
        leftPadding: 12
        rightPadding: 12
        background: Rectangle {
            radius: 8
            color: root.colBg
            border.color: parent.activeFocus ? root.colPrimary : root.colBorder
            border.width: parent.activeFocus ? 2 : 1
        }
    }

    component WbButton: Rectangle {
        id: btn
        Layout.fillWidth: true
        height: 40
        radius: 8
        property string text: ""
        property bool accent: false
        property bool enabled: true
        signal clicked

        color: !enabled       ? root.colBorder
             : accent         ? root.colPrimary
                              : root.colSurface
        border.color: accent || !enabled ? "transparent" : root.colBorder

        Behavior on color { ColorAnimation { duration: 100 } }

        Label {
            anchors.centerIn: parent
            text: btn.text
            color: btn.enabled ? "#fff" : root.colMuted
            font { pixelSize: 14; bold: btn.accent }
        }

        MouseArea {
            anchors.fill: parent
            enabled: btn.enabled
            cursorShape: btn.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: if (btn.enabled) btn.clicked()
        }
    }
}
