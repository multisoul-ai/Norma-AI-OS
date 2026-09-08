package ai.multisoul.norma.quest

import androidx.compose.ui.platform.ComposeView
import com.meta.spatial.compose.ComposeFeature
import com.meta.spatial.compose.ComposeViewPanelRegistration
import com.meta.spatial.core.Entity
import com.meta.spatial.core.Pose
import com.meta.spatial.core.Quaternion
import com.meta.spatial.core.SpatialFeature
import com.meta.spatial.core.Vector3
import com.meta.spatial.runtime.ReferenceSpace
import com.meta.spatial.toolkit.AppSystemActivity
import com.meta.spatial.toolkit.DpPerMeterDisplayOptions
import com.meta.spatial.toolkit.Grabbable
import com.meta.spatial.toolkit.PanelRegistration
import com.meta.spatial.toolkit.PanelStyleOptions
import com.meta.spatial.toolkit.QuadShapeOptions
import com.meta.spatial.toolkit.Transform
import com.meta.spatial.toolkit.UIPanelSettings
import com.meta.spatial.toolkit.createPanelEntity
import com.meta.spatial.vr.VRFeature

/** Quest is a spatial client. No agent execution or repository access belongs here. */
class QuestActivity : AppSystemActivity() {
    private val connection by lazy { HostConnection() }
    private val panels = mutableListOf<Entity>()

    override fun registerFeatures(): List<SpatialFeature> = listOf(VRFeature(this), ComposeFeature())

    override fun registerPanels(): List<PanelRegistration> = listOf(
        registerPanel(R.id.workspace_panel, 1.4f, 1.05f, PanelKind.WORKSPACE),
        registerPanel(R.id.agents_panel, 0.65f, 0.82f, PanelKind.AGENTS),
        registerPanel(R.id.decisions_panel, 0.72f, 1.05f, PanelKind.DECISIONS),
    )

    private fun registerPanel(id: Int, width: Float, height: Float, kind: PanelKind) =
        ComposeViewPanelRegistration(
            id,
            composeViewCreator = { _, context ->
                ComposeView(context).apply { setContent { NormaPanel(kind, connection) } }
            },
            settingsCreator = {
                UIPanelSettings(
                    shape = QuadShapeOptions(width = width, height = height),
                    display = DpPerMeterDisplayOptions(dpPerMeter = 600f),
                    style = PanelStyleOptions(themeResourceId = R.style.Theme_NormaQuest),
                )
            },
        )

    override fun onSceneReady() {
        super.onSceneReady()
        scene.setReferenceSpace(ReferenceSpace.LOCAL_FLOOR)
        scene.enablePassthrough(true)
        if (panels.isEmpty()) {
            addPanel(R.id.workspace_panel, 0f, 1.45f, 2f, 180f)
            addPanel(R.id.agents_panel, -1.12f, 1.45f, 2.1f, 155f)
            addPanel(R.id.decisions_panel, 1.15f, 1.45f, 2.1f, 205f)
        }
    }

    private fun addPanel(id: Int, x: Float, y: Float, z: Float, yaw: Float) {
        panels += Entity.createPanelEntity(
            id, Transform(Pose(Vector3(x, y, z), Quaternion(0f, yaw, 0f))), Grabbable(),
        )
    }

    override fun onStop() {
        connection.disconnect()
        super.onStop()
    }

    override fun onDestroy() {
        connection.close()
        super.onDestroy()
    }
}
