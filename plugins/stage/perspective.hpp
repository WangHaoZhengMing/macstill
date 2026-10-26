#pragma once

// One perspective-correct GPU quad over a full-resolution compositor workbuffer.
// No CPU copies, downsampled snapshots, or column-by-column approximations.
class Perspective {
    GLuint program = 0, vao = 0, vbo = 0;
    bool failed = false;
    GLint projection = -1, sourceProjection = -1, yaw = -1;
    GLuint compile(GLenum type, const char* source) {
        GLuint shader = glCreateShader(type);
        glShaderSource(shader, 1, &source, nullptr);
        glCompileShader(shader);
        GLint ok = 0;
        glGetShaderiv(shader, GL_COMPILE_STATUS, &ok);
        if (!ok) { glDeleteShader(shader); return 0; }
        return shader;
    }
public:
    bool ready() {
        if (program) return true;
        if (failed) return false;
        const char* vertex = R"GLSL(#version 300 es
precision highp float;
layout(location=0) in vec2 corner;
uniform mat3 projection;
uniform mat3 sourceProjection;
uniform float yaw;
uniform vec2 cameraLocal;
uniform float depthRatio;
out vec2 uv;
out vec2 local;
void main() {
    float depth = 1.0 + sin(yaw) * corner.x * depthRatio;
    vec2 projected = cameraLocal + (vec2(cos(yaw) * corner.x, corner.y) - cameraLocal) / depth;
    vec3 clip = projection * vec3(projected, 1.0);
    // Homogeneous W gives perspective-correct interpolation over both triangles.
    gl_Position = vec4(clip.xy * depth, 0.0, depth);
    uv = (sourceProjection * vec3(corner, 1.0)).xy * 0.5 + 0.5;
    local = corner;
}
)GLSL";
        const char* fragment = R"GLSL(#version 300 es
precision highp float;
uniform sampler2D content;
uniform vec2 targetSize;
uniform float cornerRadius;
uniform float yaw;
in vec2 uv;
in vec2 local;
out vec4 color;
void main() {
    color = texture(content, uv);
    // Restrained far-edge shading; alpha stays premultiplied.
    color.rgb *= 1.0 - 0.12 * sin(yaw) * local.x;
    vec2 q = abs((local - 0.5) * targetSize) - targetSize * 0.5 + cornerRadius;
    float distance = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - cornerRadius;
    float aa = max(fwidth(distance), 0.001);
    color *= 1.0 - smoothstep(-aa, 0.0, distance);
}
)GLSL";
        GLuint vs = compile(GL_VERTEX_SHADER, vertex), fs = compile(GL_FRAGMENT_SHADER, fragment);
        if (!vs || !fs) {
            if (vs) glDeleteShader(vs);
            if (fs) glDeleteShader(fs);
            failed = true; return false;
        }
        GLuint candidate = glCreateProgram();
        glAttachShader(candidate, vs); glAttachShader(candidate, fs);
        glLinkProgram(candidate);
        glDeleteShader(vs); glDeleteShader(fs);
        GLint ok = 0;
        glGetProgramiv(candidate, GL_LINK_STATUS, &ok);
        if (!ok) { glDeleteProgram(candidate); failed = true; return false; }
        program = candidate;
        projection = glGetUniformLocation(program, "projection");
        sourceProjection = glGetUniformLocation(program, "sourceProjection");
        yaw = glGetUniformLocation(program, "yaw");
        GLint previousVAO = 0, previousBuffer = 0;
        glGetIntegerv(GL_VERTEX_ARRAY_BINDING, &previousVAO);
        glGetIntegerv(GL_ARRAY_BUFFER_BINDING, &previousBuffer);
        glGenVertexArrays(1, &vao); glBindVertexArray(vao);
        glGenBuffers(1, &vbo); glBindBuffer(GL_ARRAY_BUFFER, vbo);
        const float corners[] = {0,0, 0,1, 1,0, 1,1};
        glBufferData(GL_ARRAY_BUFFER, sizeof(corners), corners, GL_STATIC_DRAW);
        glEnableVertexAttribArray(0);
        glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 0, nullptr);
        glBindVertexArray(previousVAO); glBindBuffer(GL_ARRAY_BUFFER, previousBuffer);
        return true;
    }
    void draw(SP<Render::ITexture> texture, const CBox& source, const CBox& target, double angle, const Vector2D& outputSize, double radius) {
        GLint previousProgram, previousVAO, previousActive, previousTexture;
        glGetIntegerv(GL_CURRENT_PROGRAM, &previousProgram);
        glGetIntegerv(GL_VERTEX_ARRAY_BINDING, &previousVAO);
        glGetIntegerv(GL_ACTIVE_TEXTURE, &previousActive);
        glActiveTexture(GL_TEXTURE0);
        glGetIntegerv(GL_TEXTURE_BINDING_2D, &previousTexture);
        glUseProgram(program);
        const auto targetMatrix = g_pHyprRenderer->projectBoxToTarget(target).transpose().getMatrix();
        const auto sourceMatrix = g_pHyprRenderer->projectBoxToTarget(source).transpose().getMatrix();
        glUniformMatrix3fv(projection, 1, GL_FALSE, targetMatrix.data());
        glUniformMatrix3fv(sourceProjection, 1, GL_FALSE, sourceMatrix.data());
        glUniform1f(yaw, angle);
        // A shared screen camera makes upper/lower piles converge consistently.
        glUniform2f(glGetUniformLocation(program, "cameraLocal"),
                    (outputSize.x * 0.5 - target.x) / target.w,
                    (outputSize.y * 0.5 - target.y) / target.h);
        glUniform1f(glGetUniformLocation(program, "depthRatio"), target.w / (outputSize.x * 1000.0 / 896.0));
        glUniform2f(glGetUniformLocation(program, "targetSize"), target.w, target.h);
        glUniform1f(glGetUniformLocation(program, "cornerRadius"), std::min(radius, std::min(target.w, target.h) * 0.5));
        glUniform1i(glGetUniformLocation(program, "content"), 0);
        texture->bind();
        glBindVertexArray(vao);
        g_pHyprRenderer->blend(true);
        Render::GL::g_pHyprOpenGL->scissor(nullptr);
        glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
        glBindVertexArray(previousVAO);
        glBindTexture(GL_TEXTURE_2D, previousTexture);
        glActiveTexture(previousActive);
        glUseProgram(previousProgram);
    }
    void release() {
        if (!program) return;
        Render::GL::g_pHyprOpenGL->makeEGLCurrent();
        glDeleteBuffers(1, &vbo); glDeleteVertexArrays(1, &vao); glDeleteProgram(program);
        program = vao = vbo = 0;
    }
};
static Perspective perspective;
