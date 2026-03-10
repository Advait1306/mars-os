import QtQuick 2.15
import QtQuick.Layouts 1.15
import org.kde.plasma.plasmoid 2.0
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami

PlasmoidItem {
    id: root

    preferredRepresentation: fullRepresentation

    fullRepresentation: Item {
        Layout.preferredWidth: Kirigami.Units.iconSizes.medium
        Layout.preferredHeight: Kirigami.Units.iconSizes.medium

        Kirigami.Icon {
            id: marsIcon
            anchors.centerIn: parent
            width: Kirigami.Units.iconSizes.smallMedium
            height: Kirigami.Units.iconSizes.smallMedium
            source: Qt.resolvedUrl("../icons/mars-icon.svg")
            smooth: true
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor

            onClicked: {
                // Toggle the application launcher
                var launcher = null
                var applets = Plasmoid.containment.applets
                for (var i = 0; i < applets.length; i++) {
                    if (applets[i].pluginName === "org.kde.plasma.kickoff" ||
                        applets[i].pluginName === "org.kde.plasma.kicker") {
                        launcher = applets[i]
                        break
                    }
                }
            }

            onEntered: marsIcon.opacity = 0.8
            onExited: marsIcon.opacity = 1.0
        }
    }

    Plasmoid.status: PlasmaCore.Types.ActiveStatus
}
