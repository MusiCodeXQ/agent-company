import type { Container, Graphics } from 'pixi.js';

export type AgentAnimation = 'idle' | 'walk' | 'sit';
export type AgentPose = 'neutral' | 'phone' | 'thinking' | 'typing' | 'talking';

export interface AgentRig {
  root: Container;
  torso: Container;
  head: Container;
  leftArm: Container;
  rightArm: Container;
  leftLeg: Container;
  rightLeg: Container;
  phone: Graphics;
}

/**
 * Visual-only lightweight rig controller. It never mutates Zustand or reports
 * arrival to the backend, and can later be replaced by a Sprite Atlas animator.
 */
export class AgentAnimationController {
  private elapsedMs = 0;

  update(rig: AgentRig, animation: AgentAnimation, pose: AgentPose, deltaMs: number): void {
    this.elapsedMs += deltaMs;
    const walkPhase = this.elapsedMs / 105;
    const slowPhase = this.elapsedMs / 420;
    const stride = Math.sin(walkPhase);
    const bounce = Math.abs(Math.cos(walkPhase));
    this.resetRig(rig);

    if (animation === 'walk') {
      rig.root.y = -bounce * 1.7;
      rig.root.rotation = stride * 0.012;
      rig.leftLeg.rotation = stride * 0.34;
      rig.rightLeg.rotation = -stride * 0.34;
      rig.leftArm.rotation = -stride * 0.27;
      rig.rightArm.rotation = stride * 0.27;
      rig.head.rotation = -stride * 0.018;
    } else if (animation === 'sit') {
      rig.root.y = 4;
      rig.torso.y = -33;
      rig.head.y = -55;
      rig.leftLeg.position.set(-6, -17);
      rig.rightLeg.position.set(6, -17);
      rig.leftLeg.rotation = 0.58;
      rig.rightLeg.rotation = -0.58;
      rig.leftLeg.scale.y = 0.82;
      rig.rightLeg.scale.y = 0.82;
      rig.torso.y += Math.sin(slowPhase) * 0.35;
    } else {
      const breath = Math.sin(slowPhase);
      rig.root.y = breath * 0.55;
      rig.torso.scale.y = 1 + breath * 0.008;
      rig.head.y = -58 + breath * 0.28;
      rig.leftArm.rotation = 0.045 - breath * 0.015;
      rig.rightArm.rotation = -0.045 + breath * 0.015;
    }

    this.applyPose(rig, pose, animation, stride, slowPhase);
  }

  private resetRig(rig: AgentRig): void {
    rig.root.position.set(0, 0);
    rig.root.rotation = 0;
    rig.torso.position.set(0, -36);
    rig.torso.scale.set(1);
    rig.head.position.set(0, -58);
    rig.head.rotation = 0;
    rig.leftArm.position.set(-13, -45);
    rig.rightArm.position.set(13, -45);
    rig.leftArm.rotation = 0;
    rig.rightArm.rotation = 0;
    rig.leftLeg.position.set(-5, -19);
    rig.rightLeg.position.set(5, -19);
    rig.leftLeg.rotation = 0;
    rig.rightLeg.rotation = 0;
    rig.leftLeg.scale.set(1);
    rig.rightLeg.scale.set(1);
    rig.phone.visible = false;
    rig.phone.position.set(0, 0);
    rig.phone.rotation = 0;
  }

  private applyPose(
    rig: AgentRig,
    pose: AgentPose,
    animation: AgentAnimation,
    stride: number,
    slowPhase: number,
  ): void {
    if (pose === 'phone') {
      rig.rightArm.rotation = -1.18 + Math.sin(slowPhase * 1.4) * 0.025;
      rig.leftArm.rotation = animation === 'walk' ? -stride * 0.22 : 0.08;
      rig.phone.visible = true;
      rig.phone.position.set(14, -41);
      rig.phone.rotation = -0.08;
      rig.head.rotation = 0.055;
      return;
    }
    if (pose === 'typing') {
      const tap = Math.sin(this.elapsedMs / 72) * 0.07;
      rig.leftArm.rotation = -0.72 + tap;
      rig.rightArm.rotation = 0.72 - tap;
      rig.head.rotation = Math.sin(slowPhase) * 0.018;
      return;
    }
    if (pose === 'thinking') {
      rig.rightArm.rotation = -0.82 + Math.sin(slowPhase) * 0.03;
      rig.head.rotation = -0.07 + Math.sin(slowPhase * 0.7) * 0.025;
      return;
    }
    if (pose === 'talking') {
      rig.rightArm.rotation = -0.35 + Math.sin(this.elapsedMs / 180) * 0.18;
      rig.head.rotation = Math.sin(this.elapsedMs / 260) * 0.025;
    }
  }
}
